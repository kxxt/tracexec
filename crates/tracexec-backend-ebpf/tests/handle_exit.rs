use std::{
  mem::MaybeUninit,
  os::unix::{
    fs::MetadataExt,
    process::ExitStatusExt,
  },
  path::{
    Path,
    PathBuf,
  },
  process::Command,
  time::Duration,
};

use libbpf_rs::{
  RingBufferBuilder,
  skel::{
    OpenSkel,
    Skel,
    SkelBuilder,
  },
};
use nix::{
  sys::{
    signal::{
      Signal,
      kill,
      raise,
    },
    wait::{
      WaitPidFlag,
      WaitStatus,
      waitpid,
    },
  },
  unistd::{
    ForkResult,
    Pid,
    fork,
  },
};
use rstest::{
  fixture,
  rstest,
};
use serial_test::file_serial;
use tracexec_backend_ebpf::{
  bpf::skel::{
    TracexecSystemSkel,
    TracexecSystemSkelBuilder,
    types::{
      event_type,
      exit_event,
    },
  },
  function_name,
  test_utils::{
    disable_all_programs,
    find_sh,
    prepare_handle_exit_only,
    with_skel,
  },
};

mod common;

use common::EventSlot;

#[fixture]
fn sh_executable() -> PathBuf {
  find_sh()
}

struct ExitCapture {
  pid: i32,
  event: exit_event,
}

fn run_exit_and_capture(
  skel: &TracexecSystemSkel<'_>,
  sh_executable: &Path,
  exit_code: i32,
  timeout: Duration,
) -> color_eyre::Result<ExitCapture> {
  let event_slot = EventSlot::<exit_event>::new();

  let mut rb_builder = RingBufferBuilder::new();
  let slot = event_slot.clone();
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg(format!("exit {exit_code}"))
    .spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    slot.store_matching(data, |evt| evt.header.pid == child_pid);
    0
  })?;
  let rb = rb_builder.build()?;

  let _status = child.wait()?;

  let event = event_slot.wait(&rb, timeout, "missing exit event for child")?;
  Ok(ExitCapture {
    pid: child_pid,
    event,
  })
}

struct SignalCapture {
  exit: ExitCapture,
  status: std::process::ExitStatus,
}

fn run_killed_and_capture(
  skel: &TracexecSystemSkel<'_>,
  sh_executable: &Path,
  signal: Signal,
  timeout: Duration,
) -> color_eyre::Result<SignalCapture> {
  let event_slot = EventSlot::<exit_event>::new();

  let mut rb_builder = RingBufferBuilder::new();
  let slot = event_slot.clone();
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg("sleep 20")
    .spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    slot.store_matching(data, |evt| evt.header.pid == child_pid);
    0
  })?;
  let rb = rb_builder.build()?;

  kill(Pid::from_raw(child_pid), signal)?;
  let status = child.wait()?;
  assert_eq!(status.signal(), Some(signal as i32));

  let event = event_slot.wait(&rb, timeout, "missing exit event for child")?;
  Ok(SignalCapture {
    exit: ExitCapture {
      pid: child_pid,
      event,
    },
    status,
  })
}

fn run_configured_tracee_exit_without_exec(timeout: Duration) -> color_eyre::Result<ExitCapture> {
  let event_slot = EventSlot::<exit_event>::new();

  // SAFETY: the child immediately stops itself and then exits after the parent
  // attaches BPF programs.
  let child_pid = match unsafe { fork()? } {
    ForkResult::Child => {
      raise(Signal::SIGSTOP).unwrap();
      std::process::exit(23);
    }
    ForkResult::Parent { child } => child,
  };

  let stopped = waitpid(child_pid, Some(WaitPidFlag::WSTOPPED))?;
  assert_eq!(stopped, WaitStatus::Stopped(child_pid, Signal::SIGSTOP));

  let mut obj = MaybeUninit::uninit();
  let builder = TracexecSystemSkelBuilder::default();
  let mut open_skel = builder.open(&mut obj)?;
  disable_all_programs(&mut open_skel);
  open_skel.progs.handle_exit.set_autoload(true);
  open_skel.progs.handle_exit.set_autoattach(true);
  let pid_ns_ino = std::fs::metadata("/proc/self/ns/pid")?.ino();
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(true);
    rodata.tracexec_config.tracee_pid = child_pid.as_raw();
    rodata.tracexec_config.tracee_pidns_inum = pid_ns_ino as u32;
  }
  let mut skel = open_skel.load()?;
  skel.attach()?;

  let mut rb_builder = RingBufferBuilder::new();
  let slot = event_slot.clone();
  rb_builder.add(&skel.maps.events, move |data| {
    slot.store_matching(data, |evt| evt.header.pid == child_pid.as_raw());
    0
  })?;
  let rb = rb_builder.build()?;

  kill(child_pid, Signal::SIGCONT)?;
  assert_eq!(waitpid(child_pid, None)?, WaitStatus::Exited(child_pid, 23));

  let event = event_slot.wait(&rb, timeout, "missing exit event for pre-exec root tracee")?;
  Ok(ExitCapture {
    pid: child_pid.as_raw(),
    event,
  })
}

#[rstest]
#[case::success(0)]
#[case::code_7(7)]
#[case::code_42(42)]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_handle_exit_emits_exit_event_for_exit_codes(
  #[case] exit_code: i32,
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_handle_exit_only, |skel| {
    let capture = run_exit_and_capture(skel, &sh_executable, exit_code, Duration::from_secs(2))?;
    assert_eq!(capture.event.header.r#type, event_type::EXIT_EVENT);
    assert_eq!(capture.event.header.pid, capture.pid);
    assert_eq!(capture.event.code, exit_code);
    assert_eq!(capture.event.sig, 0);
    Ok(())
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_handle_exit_marks_configured_tracee_that_exits_before_exec() -> color_eyre::Result<()> {
  let capture = run_configured_tracee_exit_without_exec(Duration::from_secs(2))?;
  assert_eq!(capture.event.header.r#type, event_type::EXIT_EVENT);
  assert_eq!(capture.event.header.pid, capture.pid);
  assert_eq!(capture.event.code, 23);
  assert_eq!(capture.event.sig, 0);
  assert!(unsafe { capture.event.is_root_tracee.assume_init() });
  Ok(())
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_handle_exit_emits_multiple_events_in_sequence(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_handle_exit_only, |skel| {
    let first = run_exit_and_capture(skel, &sh_executable, 1, Duration::from_secs(2))?;
    let second = run_exit_and_capture(skel, &sh_executable, 2, Duration::from_secs(2))?;
    assert_eq!(first.event.code, 1);
    assert_eq!(second.event.code, 2);
    Ok(())
  })
}

#[rstest]
#[case::sigkill(Signal::SIGKILL)]
#[case::sigterm(Signal::SIGTERM)]
#[case::sigint(Signal::SIGINT)]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_handle_exit_emits_signal_for_killed_tracee(
  #[case] signal: Signal,
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_handle_exit_only, |skel| {
    let capture = run_killed_and_capture(skel, &sh_executable, signal, Duration::from_secs(2))?;
    assert_eq!(capture.exit.event.header.r#type, event_type::EXIT_EVENT);
    assert_eq!(capture.exit.event.header.pid, capture.exit.pid);
    assert_eq!(capture.exit.event.sig, signal as u32);
    assert_eq!(capture.exit.event.code, 0);
    assert_eq!(capture.status.signal(), Some(signal as i32));
    Ok(())
  })
}
