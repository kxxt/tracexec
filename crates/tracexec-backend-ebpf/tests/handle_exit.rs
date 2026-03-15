use std::{
  env,
  mem::MaybeUninit,
  os::unix::process::ExitStatusExt,
  path::PathBuf,
  process::Command,
  sync::{
    Arc,
    Mutex,
  },
  time::{
    Duration,
    Instant,
  },
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
  sys::signal::{
    Signal,
    kill,
  },
  unistd::Pid,
};
use rstest::{
  fixture,
  rstest,
};
use tracexec_backend_ebpf::bpf::skel::{
  OpenTracexecSystemSkel,
  TracexecSystemSkel,
  TracexecSystemSkelBuilder,
  types::{
    event_type,
    exit_event,
  },
};

#[fixture]
fn sh_executable() -> PathBuf {
  env::var_os("PATH")
    .and_then(|paths| {
      env::split_paths(&paths)
        .filter_map(|dir| {
          let full_path = dir.join("sh");
          if full_path.is_file() {
            Some(full_path)
          } else {
            None
          }
        })
        .next()
    })
    .expect("executable `sh` not found")
}

struct ExitCapture {
  pid: i32,
  event: exit_event,
}

fn disable_all_programs(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  for mut prog in open_skel.open_object_mut().progs_mut() {
    prog.set_autoload(false);
  }
}

fn prepare_handle_exit_only(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.handle_exit.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

fn with_handle_exit_skel<T>(
  f: impl for<'obj> FnOnce(&mut TracexecSystemSkel<'obj>) -> color_eyre::Result<T>,
) -> color_eyre::Result<T> {
  let mut obj = MaybeUninit::uninit();
  let builder = TracexecSystemSkelBuilder::default();
  let mut open_skel = builder.open(&mut obj)?;
  prepare_handle_exit_only(&mut open_skel);
  let mut skel = open_skel.load()?;
  skel.attach()?;
  f(&mut skel)
}

fn run_exit_and_capture(
  skel: &mut TracexecSystemSkel<'_>,
  sh_executable: &PathBuf,
  exit_code: i32,
  timeout: Duration,
) -> color_eyre::Result<ExitCapture> {
  let event_slot: Arc<Mutex<Option<exit_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg(format!("exit {exit_code}"))
    .spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<exit_event>() {
      // SAFETY: exit_event is a plain old data struct produced by the eBPF program.
      //         bpf ringbuf sample is 8 byte aligned.
      let evt = unsafe { std::ptr::read(data.as_ptr() as *const exit_event) };
      if evt.header.pid == child_pid {
        *slot.lock().unwrap() = Some(evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let _status = child.wait()?;

  let start = Instant::now();
  while start.elapsed() < timeout {
    rb.poll(Duration::from_millis(50))?;
    if event_slot.lock().unwrap().is_some() {
      break;
    }
  }

  let event = event_slot
    .lock()
    .unwrap()
    .expect("missing exit event for child");
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
  skel: &mut TracexecSystemSkel<'_>,
  sh_executable: &PathBuf,
  signal: Signal,
  timeout: Duration,
) -> color_eyre::Result<SignalCapture> {
  let event_slot: Arc<Mutex<Option<exit_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg("sleep 20")
    .spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<exit_event>() {
      // SAFETY: exit_event is a plain old data struct produced by the eBPF program.
      //         bpf ringbuf sample is 8 byte aligned.
      let evt = unsafe { std::ptr::read(data.as_ptr() as *const exit_event) };
      if evt.header.pid == child_pid {
        *slot.lock().unwrap() = Some(evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  kill(Pid::from_raw(child_pid), signal)?;
  let status = child.wait()?;
  assert_eq!(status.signal(), Some(signal as i32));

  let start = Instant::now();
  while start.elapsed() < timeout {
    rb.poll(Duration::from_millis(50))?;
    if event_slot.lock().unwrap().is_some() {
      break;
    }
  }

  let event = event_slot
    .lock()
    .unwrap()
    .expect("missing exit event for child");
  Ok(SignalCapture {
    exit: ExitCapture {
      pid: child_pid,
      event,
    },
    status,
  })
}

#[rstest]
#[ignore = "root"]
fn test_handle_exit_emits_exit_event_for_exit_codes(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  let exit_codes = [0, 7, 42];
  with_handle_exit_skel(|skel| {
    for exit_code in exit_codes {
      let capture = run_exit_and_capture(skel, &sh_executable, exit_code, Duration::from_secs(2))?;
      assert_eq!(capture.event.header.r#type, event_type::EXIT_EVENT);
      assert_eq!(capture.event.header.pid, capture.pid);
      assert_eq!(capture.event.code, exit_code);
      assert_eq!(capture.event.sig, 0);
    }
    Ok(())
  })
}

#[rstest]
#[ignore = "root"]
fn test_handle_exit_emits_multiple_events_in_sequence(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_handle_exit_skel(|skel| {
    let first = run_exit_and_capture(skel, &sh_executable, 1, Duration::from_secs(2))?;
    let second = run_exit_and_capture(skel, &sh_executable, 2, Duration::from_secs(2))?;
    assert_eq!(first.event.code, 1);
    assert_eq!(second.event.code, 2);
    Ok(())
  })
}

#[rstest]
#[ignore = "root"]
fn test_handle_exit_emits_signal_for_killed_tracee(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  let signals = [Signal::SIGKILL, Signal::SIGTERM, Signal::SIGINT];
  with_handle_exit_skel(|skel| {
    for signal in signals {
      let capture = run_killed_and_capture(skel, &sh_executable, signal, Duration::from_secs(2))?;
      assert_eq!(capture.exit.event.header.r#type, event_type::EXIT_EVENT);
      assert_eq!(capture.exit.event.header.pid, capture.exit.pid);
      assert_eq!(capture.exit.event.sig, signal as u32);
      assert_eq!(capture.exit.event.code, 0);
      assert_eq!(capture.status.signal(), Some(signal as i32));
    }
    Ok(())
  })
}
