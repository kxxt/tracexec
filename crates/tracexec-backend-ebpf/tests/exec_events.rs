use std::{
  ffi::CString,
  os::unix::ffi::OsStrExt,
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

use libbpf_rs::RingBufferBuilder;
use nix::{
  sys::wait::{
    WaitStatus,
    waitpid,
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
  bpf::{
    interface::BpfEventFlags,
    skel::types::{
      event_type,
      exec_event,
      fd_event,
      path_event,
      tracexec_event_header,
    },
    utf8_lossy_cow_from_bytes_with_nul,
  },
  function_name,
  parser::{
    parse_groups_event,
    parse_path_segment,
    parse_string_event,
  },
  test_utils::{
    find_sh,
    prepare_execve_fentry_fexit,
    prepare_execve_kprobe_kretprobe,
    prepare_execveat_fentry_fexit,
    prepare_execveat_kprobe_kretprobe,
    with_skel,
  },
};

#[fixture]
fn sh_executable() -> PathBuf {
  find_sh()
}

struct ExecCapture {
  pid: i32,
  event: exec_event,
}

struct AuxCapture {
  pid: i32,
  exec: exec_event,
  strings: Vec<String>,
  path_events: Vec<path_event>,
  path_segments: Vec<String>,
  fd_events: Vec<fd_event>,
  groups_sizes: Vec<usize>,
}

fn run_command_and_capture(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  mut cmd: Command,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let event_slot: Arc<Mutex<Option<exec_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let mut child = cmd.spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<exec_event>() {
      // SAFETY: exec_event is a plain old data struct produced by the eBPF program.
      //         bpf ringbuf sample is 8 byte aligned.
      let evt = unsafe { std::ptr::read(data.as_ptr() as *const exec_event) };
      if evt.header.pid == child_pid && evt.header.r#type == event_type::SYSEXIT_EVENT {
        *slot.lock().unwrap() = Some(evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let status = child.wait()?;
  assert!(status.success());

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
    .expect("missing exec event for child");
  Ok(ExecCapture {
    pid: child_pid,
    event,
  })
}

fn run_exec_and_capture(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  sh_executable: &PathBuf,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let mut cmd = Command::new(sh_executable);
  cmd.arg("-c").arg("true");
  run_command_and_capture(skel, cmd, timeout)
}

#[allow(unused)]
fn run_binary_and_capture(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &PathBuf,
  args: &[&str],
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let mut cmd = Command::new(exe);
  cmd.args(args);
  run_command_and_capture(skel, cmd, timeout)
}

fn run_execveat_and_capture(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &PathBuf,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let event_slot: Arc<Mutex<Option<exec_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let sh_path = exe.clone();
  let sh_dir = sh_path
    .parent()
    .expect("sh has no parent directory")
    .to_path_buf();
  let sh_name = sh_path
    .file_name()
    .expect("sh has no file name")
    .to_os_string();
  // SAFETY: this test forks and immediately execs/waits without sharing mutable state.
  let child_pid = match unsafe { fork()? } {
    ForkResult::Child => {
      let name_c = CString::new(sh_name.as_os_str().as_bytes()).unwrap();
      let dirfd = nix::fcntl::open(
        &sh_dir,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_DIRECTORY,
        nix::sys::stat::Mode::empty(),
      )
      .unwrap();

      match nix::unistd::execveat(
        dirfd,
        &name_c,
        &[c"sh", c"-c", c"true"],
        &[c"A=B"],
        nix::fcntl::AtFlags::empty(),
      ) {
        Ok(never) => match never {},
        Err(e) => panic!("execveat failed in child: {e}"),
      }
    }
    ForkResult::Parent { child } => child.as_raw(),
  };

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<exec_event>() {
      // SAFETY: exec_event is a plain old data struct produced by the eBPF program.
      //         bpf ringbuf sample is 8 byte aligned.
      let evt = unsafe { std::ptr::read(data.as_ptr() as *const exec_event) };
      if evt.header.pid == child_pid && evt.header.r#type == event_type::SYSEXIT_EVENT {
        *slot.lock().unwrap() = Some(evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let status = waitpid(Pid::from_raw(child_pid), None)?;
  assert_eq!(status, WaitStatus::Exited(Pid::from_raw(child_pid), 0));

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
    .expect("missing exec event for child");
  Ok(ExecCapture {
    pid: child_pid,
    event,
  })
}

fn run_execveat_in_thread_and_capture(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &PathBuf,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let event_slot: Arc<Mutex<Option<exec_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let sh_path = exe.clone();
  let sh_dir = sh_path
    .parent()
    .expect("sh has no parent directory")
    .to_path_buf();
  let sh_name = sh_path
    .file_name()
    .expect("sh has no file name")
    .to_os_string();

  // SAFETY: this test forks and immediately coordinates an exec path in child.
  let child_pid = match unsafe { fork()? } {
    ForkResult::Child => {
      let join = std::thread::spawn(move || {
        let name_c = CString::new(sh_name.as_os_str().as_bytes()).unwrap();

        let dirfd = nix::fcntl::open(
          &sh_dir,
          nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_DIRECTORY,
          nix::sys::stat::Mode::empty(),
        )
        .unwrap();

        nix::unistd::execveat(
          dirfd,
          &name_c,
          &[c"sh", c"-c", c"true"],
          &[c"A=B"],
          nix::fcntl::AtFlags::empty(),
        )
        .unwrap();

        unreachable!("execveat helper thread should not return");
      });

      let _ = join.join();
      panic!("execveat from non-main thread did not replace process image");
    }
    ForkResult::Parent { child } => child.as_raw(),
  };

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<exec_event>() {
      // SAFETY: exec_event is a plain old data struct produced by the eBPF program.
      //         bpf ringbuf sample is 8 byte aligned.
      let evt = unsafe { std::ptr::read(data.as_ptr() as *const exec_event) };
      if evt.tgid == child_pid
        && evt.header.r#type == event_type::SYSEXIT_EVENT
        && evt.header.pid != evt.tgid
      {
        *slot.lock().unwrap() = Some(evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let status = waitpid(Pid::from_raw(child_pid), None)?;
  assert_eq!(status, WaitStatus::Exited(Pid::from_raw(child_pid), 0));

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
    .expect("missing non-main-thread exec event for child");
  Ok(ExecCapture {
    pid: child_pid,
    event,
  })
}

fn run_exec_and_collect_aux(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  sh_executable: &PathBuf,
  timeout: Duration,
) -> color_eyre::Result<AuxCapture> {
  #[derive(Default)]
  struct State {
    exec: Option<exec_event>,
    strings: Vec<String>,
    path_events: Vec<path_event>,
    path_segments: Vec<String>,
    fd_events: Vec<fd_event>,
    groups_sizes: Vec<usize>,
  }

  let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State::default()));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&state);
  let mut child = Command::new(sh_executable).arg("-c").arg("true").spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() < std::mem::size_of::<tracexec_event_header>() {
      return 0;
    }
    // SAFETY: header is plain old data and ringbuf sample is 8 byte aligned.
    let header = unsafe { std::ptr::read(data.as_ptr() as *const tracexec_event_header) };
    if header.pid != child_pid {
      return 0;
    }
    let mut guard = slot.lock().unwrap();
    match header.r#type {
      event_type::SYSEXIT_EVENT => {
        if data.len() == std::mem::size_of::<exec_event>() {
          // SAFETY: exec_event is a plain old data struct produced by the eBPF program.
          let evt = unsafe { std::ptr::read(data.as_ptr() as *const exec_event) };
          guard.exec = Some(evt);
        }
      }
      event_type::STRING_EVENT => {
        let msg = parse_string_event(&header, data);
        guard.strings.push(msg.as_ref().to_string());
      }
      event_type::PATH_EVENT => {
        if data.len() == std::mem::size_of::<path_event>() {
          // SAFETY: path_event is a plain old data struct produced by the eBPF program.
          let evt = unsafe { std::ptr::read(data.as_ptr() as *const path_event) };
          guard.path_events.push(evt);
        }
      }
      event_type::PATH_SEGMENT_EVENT => {
        let msg = parse_path_segment(data);
        guard.path_segments.push(msg.as_ref().to_string());
      }
      event_type::FD_EVENT => {
        if data.len() == std::mem::size_of::<fd_event>() {
          // SAFETY: fd_event is a plain old data struct produced by the eBPF program.
          let evt = unsafe { std::ptr::read(data.as_ptr() as *const fd_event) };
          guard.fd_events.push(evt);
        }
      }
      event_type::GROUPS_EVENT => {
        let groups = parse_groups_event(data);
        guard.groups_sizes.push(groups.len());
      }
      _ => {}
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let status = child.wait()?;
  assert!(status.success());

  let start = Instant::now();
  while start.elapsed() < timeout {
    rb.poll(Duration::from_millis(50))?;
    if state.lock().unwrap().exec.is_some() {
      break;
    }
  }

  let guard = state.lock().unwrap();
  let exec = guard.exec.expect("missing exec event for child");
  Ok(AuxCapture {
    pid: child_pid,
    exec,
    strings: guard.strings.clone(),
    path_events: guard.path_events.clone(),
    path_segments: guard.path_segments.clone(),
    fd_events: guard.fd_events.clone(),
    groups_sizes: guard.groups_sizes.clone(),
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_execve_kprobe_kretprobe_emits_exec_event(sh_executable: PathBuf) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_execve_kprobe_kretprobe, |skel| {
    let capture = run_exec_and_capture(skel, &sh_executable, Duration::from_secs(2))?;
    let is_execveat = unsafe { capture.event.is_execveat.assume_init() };
    assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
    assert_eq!(capture.event.header.pid, capture.pid);
    assert!(!is_execveat);
    assert_eq!(capture.event.header.pid, capture.event.tgid);
    assert_eq!(capture.event.ret, 0);
    Ok(())
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_execve_fentry_fexit_emits_exec_event(sh_executable: PathBuf) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_execve_fentry_fexit, |skel| {
    let capture = run_exec_and_capture(skel, &sh_executable, Duration::from_secs(4))?;
    let is_execveat = unsafe { capture.event.is_execveat.assume_init() };
    assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
    assert_eq!(capture.event.header.pid, capture.pid);
    assert!(!is_execveat);
    assert_eq!(capture.event.header.pid, capture.event.tgid);
    assert_eq!(capture.event.ret, 0);
    Ok(())
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_execveat_kprobe_kretprobe_emits_exec_event(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_skel(
    function_name!(),
    prepare_execveat_kprobe_kretprobe,
    |skel| {
      let capture = run_execveat_and_capture(skel, &sh_executable, Duration::from_secs(4))?;
      let is_execveat = unsafe { capture.event.is_execveat.assume_init() };
      assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
      assert_eq!(capture.event.header.pid, capture.pid);
      assert!(is_execveat);
      assert_eq!(capture.event.header.pid, capture.event.tgid);
      assert_eq!(capture.event.ret, 0);
      Ok(())
    },
  )
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_execveat_fentry_fexit_emits_exec_event(sh_executable: PathBuf) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_execveat_fentry_fexit, |skel| {
    let capture = run_execveat_and_capture(skel, &sh_executable, Duration::from_secs(4))?;
    let is_execveat = unsafe { capture.event.is_execveat.assume_init() };
    assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
    assert_eq!(capture.event.header.pid, capture.pid);
    assert!(is_execveat);
    assert_eq!(capture.event.header.pid, capture.event.tgid);
    assert_eq!(capture.event.ret, 0);
    Ok(())
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_execveat_from_non_main_thread_emits_non_main_exec_pid(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_skel(
    function_name!(),
    prepare_execveat_kprobe_kretprobe,
    |skel| {
      let capture =
        run_execveat_in_thread_and_capture(skel, &sh_executable, Duration::from_secs(4))?;
      let is_execveat = unsafe { capture.event.is_execveat.assume_init() };
      assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
      assert!(is_execveat);
      assert_eq!(capture.event.tgid, capture.pid);
      assert_ne!(capture.event.header.pid, capture.event.tgid);
      assert_eq!(capture.event.ret, 0);
      Ok(())
    },
  )
}

#[cfg(target_arch = "x86_64")]
#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_compat_execve_emits_exec_event() -> color_eyre::Result<()> {
  use tracexec_backend_ebpf::test_utils::prepare_compat_execve;
  let bin = PathBuf::from(env!("CARGO_BIN_EXE_compat-exec"));
  with_skel(function_name!(), prepare_compat_execve, |skel| {
    let capture = run_binary_and_capture(skel, &bin, &[], Duration::from_secs(4))?;
    assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
    assert_eq!(capture.event.header.pid, capture.pid);
    assert_eq!(capture.event.ret, 0);
    Ok(())
  })
}

#[cfg(target_arch = "x86_64")]
#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_compat_execveat_emits_exec_event() -> color_eyre::Result<()> {
  use tracexec_backend_ebpf::test_utils::prepare_compat_execveat;
  let bin = PathBuf::from(env!("CARGO_BIN_EXE_compat-exec"));
  with_skel(function_name!(), prepare_compat_execveat, |skel| {
    let capture = run_binary_and_capture(skel, &bin, &["execveat"], Duration::from_secs(4))?;
    assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
    assert_eq!(capture.event.header.pid, capture.pid);
    assert_eq!(capture.event.ret, 0);
    Ok(())
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_exec_emits_auxiliary_events(sh_executable: PathBuf) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_execve_kprobe_kretprobe, |skel| {
    let capture = run_exec_and_collect_aux(skel, &sh_executable, Duration::from_secs(4))?;
    assert_eq!(capture.exec.header.r#type, event_type::SYSEXIT_EVENT);
    assert_eq!(capture.exec.header.pid, capture.pid);
    assert_eq!(capture.exec.ret, 0);
    assert!(!capture.strings.is_empty(), "expected STRING_EVENTs");
    assert!(
      capture.strings.iter().any(|s| s == "true"),
      "expected STRING_EVENT containing argv 'true'"
    );
    assert!(
      capture.strings.iter().any(|s| s.starts_with("PATH=")),
      "expected STRING_EVENT containing PATH env"
    );
    assert!(!capture.path_events.is_empty(), "expected PATH_EVENTs");
    assert!(
      !capture.path_segments.is_empty(),
      "expected PATH_SEGMENT_EVENTs"
    );
    assert!(
      capture.path_segments.iter().any(|s| !s.is_empty()),
      "expected non-empty path segments"
    );
    assert!(!capture.fd_events.is_empty(), "expected FD_EVENTs");
    assert!(
      capture
        .fd_events
        .iter()
        .all(|e| !utf8_lossy_cow_from_bytes_with_nul(e.fstype.as_slice()).is_empty()),
      "expected FD_EVENT with non-empty fstype"
    );
    let cred_err = (capture.exec.header.flags & (BpfEventFlags::CRED_READ_ERR as u32)) != 0;
    if !cred_err {
      assert!(!capture.groups_sizes.is_empty(), "expected GROUPS_EVENTs");
      assert!(
        capture.groups_sizes.iter().any(|s| *s > 0),
        "expected non-empty GROUPS_EVENT payload"
      );
    }
    Ok(())
  })
}
