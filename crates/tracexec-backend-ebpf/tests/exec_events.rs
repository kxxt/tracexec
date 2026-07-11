use std::{
  ffi::CString,
  os::unix::ffi::OsStrExt,
  path::{
    Path as FsPath,
    PathBuf,
  },
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
    skel::{
      OpenTracexecSystemSkel,
      types::{
        event_type,
        exec_event,
        fd_event,
        path_event,
        path_segment_event,
        tracexec_event_header,
      },
    },
    utf8_lossy_cow_from_bytes_with_nul,
  },
  event::Path,
  function_name,
  parser::{
    parse_groups_event,
    parse_path_segment,
    parse_string_event,
    process_path,
  },
  probe::{
    kernel_have_ftrace_with_direct_calls,
    kernel_rejects_syscall_wrapper_kprobes,
  },
  test_utils::{
    KCONFIG,
    LoadedSkelCallback,
    find_sh,
    prepare_execve_fentry_fexit,
    prepare_execve_kprobe_kretprobe,
    prepare_execveat_fentry_fexit,
    prepare_execveat_kprobe_kretprobe,
    with_skel,
  },
};
use tracexec_core::event::{
  BpfError,
  OutputMsg,
};

#[fixture]
fn sh_executable() -> PathBuf {
  find_sh()
}

struct ExecCapture {
  pid: i32,
  event: exec_event,
}

type PrepareSkel =
  for<'obj> fn(&mut OpenTracexecSystemSkel<'obj>) -> Option<Box<LoadedSkelCallback>>;

#[derive(Clone, Copy, Debug)]
enum ExecProbe {
  ExecveKprobeKretprobe,
  ExecveFentryFexit,
  ExecveatKprobeKretprobe,
  ExecveatFentryFexit,
}

impl ExecProbe {
  fn prepare(self) -> PrepareSkel {
    match self {
      ExecProbe::ExecveKprobeKretprobe => prepare_execve_kprobe_kretprobe,
      ExecProbe::ExecveFentryFexit => prepare_execve_fentry_fexit,
      ExecProbe::ExecveatKprobeKretprobe => prepare_execveat_kprobe_kretprobe,
      ExecProbe::ExecveatFentryFexit => prepare_execveat_fentry_fexit,
    }
  }

  fn is_execveat(self) -> bool {
    matches!(
      self,
      ExecProbe::ExecveatKprobeKretprobe | ExecProbe::ExecveatFentryFexit
    )
  }

  fn is_fentry(self) -> bool {
    matches!(
      self,
      ExecProbe::ExecveFentryFexit | ExecProbe::ExecveatFentryFexit
    )
  }

  fn test_suffix(self) -> &'static str {
    match self {
      ExecProbe::ExecveKprobeKretprobe => "execve_kprobe_kretprobe",
      ExecProbe::ExecveFentryFexit => "execve_fentry_fexit",
      ExecProbe::ExecveatKprobeKretprobe => "execveat_kprobe_kretprobe",
      ExecProbe::ExecveatFentryFexit => "execveat_fentry_fexit",
    }
  }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
enum CompatExecProbe {
  Execve,
  Execveat,
}

#[cfg(target_arch = "x86_64")]
impl CompatExecProbe {
  fn prepare(self) -> PrepareSkel {
    use tracexec_backend_ebpf::test_utils::{
      prepare_compat_execve,
      prepare_compat_execveat,
    };

    match self {
      CompatExecProbe::Execve => prepare_compat_execve,
      CompatExecProbe::Execveat => prepare_compat_execveat,
    }
  }

  fn args(self) -> &'static [&'static str] {
    match self {
      CompatExecProbe::Execve => &[],
      CompatExecProbe::Execveat => &["execveat"],
    }
  }

  fn test_suffix(self) -> &'static str {
    match self {
      CompatExecProbe::Execve => "execve",
      CompatExecProbe::Execveat => "execveat",
    }
  }
}

struct AuxCapture {
  pid: i32,
  exec: exec_event,
  exec_events: Vec<exec_event>,
  strings: Vec<String>,
  path_events: Vec<path_event>,
  path_segments: Vec<String>,
  raw_path_segments: Vec<PathSegmentCapture>,
  fd_events: Vec<fd_event>,
  groups_sizes: Vec<usize>,
}

#[derive(Clone)]
struct PathSegmentCapture {
  eid: u64,
  path_id: i32,
  index: usize,
  segment: OutputMsg,
}

fn run_command_and_capture(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
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
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  sh_executable: &FsPath,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let mut cmd = Command::new(sh_executable);
  cmd.arg("-c").arg("true");
  run_command_and_capture(skel, cmd, timeout)
}

fn assert_main_thread_exec_event(capture: &ExecCapture, is_execveat: bool) {
  let actual_is_execveat = unsafe { capture.event.is_execveat.assume_init() };
  assert_eq!(capture.event.header.r#type, event_type::SYSEXIT_EVENT);
  assert_eq!(capture.event.header.pid, capture.pid);
  assert_eq!(actual_is_execveat, is_execveat);
  assert_eq!(capture.event.header.pid, capture.event.tgid);
  assert_eq!(capture.event.ret, 0);
}

#[allow(unused)]
fn run_binary_and_capture(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &FsPath,
  args: &[&str],
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let mut cmd = Command::new(exe);
  cmd.args(args);
  run_command_and_capture(skel, cmd, timeout)
}

fn run_execveat_and_capture(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &FsPath,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let event_slot: Arc<Mutex<Option<exec_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let sh_path = exe.to_path_buf();
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
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &FsPath,
  timeout: Duration,
) -> color_eyre::Result<ExecCapture> {
  let event_slot: Arc<Mutex<Option<exec_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let sh_path = exe.to_path_buf();
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

fn run_command_and_collect_aux(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  mut cmd: Command,
  timeout: Duration,
) -> color_eyre::Result<AuxCapture> {
  #[derive(Default)]
  struct State {
    exec: Option<exec_event>,
    exec_events: Vec<exec_event>,
    strings: Vec<String>,
    path_events: Vec<path_event>,
    path_segments: Vec<String>,
    raw_path_segments: Vec<PathSegmentCapture>,
    fd_events: Vec<fd_event>,
    groups_sizes: Vec<usize>,
  }

  impl State {
    fn event_count(&self) -> usize {
      self.exec_events.len()
        + self.strings.len()
        + self.path_events.len()
        + self.raw_path_segments.len()
        + self.fd_events.len()
        + self.groups_sizes.len()
    }
  }

  let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State::default()));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&state);
  let mut child = cmd.spawn()?;
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
          guard.exec_events.push(evt);
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
        if data.len() == std::mem::size_of::<path_segment_event>() {
          // SAFETY: path_segment_event is a plain old data struct produced by the eBPF program.
          let evt = unsafe { std::ptr::read(data.as_ptr() as *const path_segment_event) };
          guard.raw_path_segments.push(PathSegmentCapture {
            eid: header.eid,
            path_id: header.id as i32,
            index: evt.index as usize,
            segment: msg.clone(),
          });
        }
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

  const POLL_INTERVAL: Duration = Duration::from_millis(50);
  const QUIET_POLLS_TO_ASSUME_DRAINED: usize = 4;

  let start = Instant::now();
  let mut quiet_polls_after_exec = 0;
  while start.elapsed() < timeout {
    let before = state.lock().unwrap().event_count();
    rb.poll(POLL_INTERVAL)?;
    let guard = state.lock().unwrap();
    let after = guard.event_count();
    if guard.exec.is_some() && after == before {
      quiet_polls_after_exec += 1;
    } else {
      quiet_polls_after_exec = 0;
    }

    // The child has exited, but ring-buffer samples can still be queued.
    // Stop only after the exec event was seen and polling has gone quiet.
    if quiet_polls_after_exec >= QUIET_POLLS_TO_ASSUME_DRAINED {
      break;
    }
  }

  let guard = state.lock().unwrap();
  let exec = guard.exec.expect("missing exec event for child");
  Ok(AuxCapture {
    pid: child_pid,
    exec,
    exec_events: guard.exec_events.clone(),
    strings: guard.strings.clone(),
    path_events: guard.path_events.clone(),
    path_segments: guard.path_segments.clone(),
    raw_path_segments: guard.raw_path_segments.clone(),
    fd_events: guard.fd_events.clone(),
    groups_sizes: guard.groups_sizes.clone(),
  })
}

fn run_exec_and_collect_aux(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  sh_executable: &FsPath,
  timeout: Duration,
) -> color_eyre::Result<AuxCapture> {
  let mut cmd = Command::new(sh_executable);
  cmd.arg("-c").arg("true");
  run_command_and_collect_aux(skel, cmd, timeout)
}

fn run_binary_and_collect_aux(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  exe: &FsPath,
  args: &[&str],
  timeout: Duration,
) -> color_eyre::Result<AuxCapture> {
  let mut cmd = Command::new(exe);
  cmd.args(args);
  run_command_and_collect_aux(skel, cmd, timeout)
}

fn fd_fstype(event: &fd_event) -> String {
  utf8_lossy_cow_from_bytes_with_nul(event.fstype.as_slice()).to_string()
}

fn fd_pseudo_name(event: &fd_event) -> String {
  utf8_lossy_cow_from_bytes_with_nul(event.pseudo_name.as_slice()).to_string()
}

fn paths_for_eid(capture: &AuxCapture, eid: u64) -> hashbrown::HashMap<i32, Path> {
  let mut paths = hashbrown::HashMap::new();
  for event in capture
    .path_events
    .iter()
    .filter(|event| event.header.eid == eid)
  {
    paths.entry(event.header.id as i32).or_insert_with(|| Path {
      is_absolute: true,
      segments: Vec::with_capacity(event.segment_count as usize),
      error: (event.header.flags != 0).then_some(BpfError::Flags.into()),
    });
  }

  for segment in capture
    .raw_path_segments
    .iter()
    .filter(|segment| segment.eid == eid)
  {
    let path = paths.entry(segment.path_id).or_insert_with(|| Path {
      is_absolute: true,
      segments: Vec::new(),
      error: None,
    });
    while path.segments.len() <= segment.index {
      path.segments.push(OutputMsg::Err(BpfError::Dropped.into()));
    }
    path.segments[segment.index] = segment.segment.clone();
  }

  paths
}

fn rendered_fd_path(capture: &AuxCapture, event: &fd_event) -> OutputMsg {
  let paths = paths_for_eid(capture, event.header.eid);
  let fs = fd_fstype(event);
  process_path(event, &fs, &paths)
}

fn with_optional_sleepable_skel(
  test_name: &str,
  prepare: impl for<'obj> FnOnce(
    &mut tracexec_backend_ebpf::bpf::skel::OpenTracexecSystemSkel<'obj>,
  ) -> Option<Box<LoadedSkelCallback>>,
  f: impl for<'obj> FnOnce(
    &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'obj>,
  ) -> color_eyre::Result<()>,
) -> color_eyre::Result<()> {
  match with_skel(test_name, prepare, f) {
    Ok(()) => Ok(()),
    Err(err) if format!("{err:?}").contains("Invalid argument (os error 22)") => {
      eprintln!("skipping {test_name}: kernel rejected sleepable fentry probe: {err}");
      Ok(())
    }
    Err(err) => Err(err),
  }
}

fn with_optional_syscall_wrapper_kprobe_skel(
  test_name: &str,
  prepare: impl for<'obj> FnOnce(
    &mut tracexec_backend_ebpf::bpf::skel::OpenTracexecSystemSkel<'obj>,
  ) -> Option<Box<LoadedSkelCallback>>,
  f: impl for<'obj> FnOnce(
    &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'obj>,
  ) -> color_eyre::Result<()>,
) -> color_eyre::Result<()> {
  if kernel_rejects_syscall_wrapper_kprobes(KCONFIG.as_ref()) {
    eprintln!("skipping {test_name}: kernel rejects syscall wrapper kprobes");
    return Ok(());
  }
  with_skel(test_name, prepare, f)
}

fn kernel_supports_ftrace_with_direct_calls() -> bool {
  kernel_have_ftrace_with_direct_calls(KCONFIG.as_ref(), None)
}

#[rstest]
#[case::execve_kprobe_kretprobe(ExecProbe::ExecveKprobeKretprobe)]
#[case::execve_fentry_fexit(ExecProbe::ExecveFentryFexit)]
#[case::execveat_kprobe_kretprobe(ExecProbe::ExecveatKprobeKretprobe)]
#[case::execveat_fentry_fexit(ExecProbe::ExecveatFentryFexit)]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_exec_probe_emits_exec_event(
  #[case] probe: ExecProbe,
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  let test_name = format!("{}_{}", function_name!(), probe.test_suffix());
  if probe.is_fentry() && !kernel_supports_ftrace_with_direct_calls() {
    eprintln!(
      "Skipping {} due to missing CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS",
      test_name
    );
    return Ok(());
  }

  let run_probe = |skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>| {
    let capture = if probe.is_execveat() {
      run_execveat_and_capture(skel, &sh_executable, Duration::from_secs(4))?
    } else {
      run_exec_and_capture(skel, &sh_executable, Duration::from_secs(2))?
    };
    assert_main_thread_exec_event(&capture, probe.is_execveat());
    Ok(())
  };

  if probe.is_fentry() {
    with_optional_sleepable_skel(&test_name, probe.prepare(), run_probe)
  } else {
    with_optional_syscall_wrapper_kprobe_skel(&test_name, probe.prepare(), run_probe)
  }
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_execveat_from_non_main_thread_emits_non_main_exec_pid(
  sh_executable: PathBuf,
) -> color_eyre::Result<()> {
  with_optional_syscall_wrapper_kprobe_skel(
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
#[case::execve(CompatExecProbe::Execve)]
#[case::execveat(CompatExecProbe::Execveat)]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_compat_exec_emits_exec_event(#[case] probe: CompatExecProbe) -> color_eyre::Result<()> {
  let bin = PathBuf::from(env!("CARGO_BIN_EXE_compat-exec"));
  let test_name = format!("{}_{}", function_name!(), probe.test_suffix());
  with_optional_sleepable_skel(&test_name, probe.prepare(), |skel| {
    let capture = run_binary_and_capture(skel, &bin, probe.args(), Duration::from_secs(4))?;
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
  with_optional_syscall_wrapper_kprobe_skel(
    function_name!(),
    prepare_execve_kprobe_kretprobe,
    |skel| {
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
    },
  )
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_exec_reports_pseudo_filesystem_fds_across_exec() -> color_eyre::Result<()> {
  let bin = PathBuf::from(env!("CARGO_BIN_EXE_special-fds-exec"));
  with_optional_syscall_wrapper_kprobe_skel(
    function_name!(),
    prepare_execve_kprobe_kretprobe,
    |skel| {
      let capture = run_binary_and_collect_aux(skel, &bin, &[], Duration::from_secs(4))?;
      assert!(
        capture.exec_events.len() >= 2,
        "fixture should exec once after opening special fds"
      );

      let reexec_eid = capture
        .exec_events
        .iter()
        .map(|event| event.header.eid)
        .max()
        .expect("missing exec events");
      let reexec_fds = capture
        .fd_events
        .iter()
        .filter(|event| event.header.eid == reexec_eid)
        .collect::<Vec<_>>();
      assert!(!reexec_fds.is_empty(), "missing fd events for re-exec");

      let pipe_fds = reexec_fds
        .iter()
        .filter(|event| fd_fstype(event) == "pipefs" && event.uses_d_dname != 0)
        .collect::<Vec<_>>();
      assert!(
        pipe_fds.len() >= 2,
        "expected both inherited pipe fds, got {}",
        pipe_fds.len()
      );
      assert!(
        pipe_fds
          .iter()
          .all(|event| rendered_fd_path(&capture, event)
            .as_ref()
            .starts_with("pipe:[")),
        "pipe fds should render as pipe:[ino]"
      );

      let socket_fds = reexec_fds
        .iter()
        .filter(|event| fd_fstype(event) == "sockfs" && event.uses_d_dname != 0)
        .collect::<Vec<_>>();
      assert!(
        socket_fds.len() >= 2,
        "expected both inherited socketpair fds, got {}",
        socket_fds.len()
      );
      assert!(
        socket_fds
          .iter()
          .all(|event| rendered_fd_path(&capture, event)
            .as_ref()
            .starts_with("socket:[")),
        "socket fds should render as socket:[ino]"
      );

      let anon_paths = reexec_fds
        .iter()
        .filter(|event| fd_fstype(event) == "anon_inodefs" && event.uses_d_dname != 0)
        .map(|event| rendered_fd_path(&capture, event).as_ref().to_string())
        .collect::<Vec<_>>();
      assert!(
        anon_paths
          .iter()
          .any(|path| path.contains("eventfd") || path.contains("eventpoll")),
        "expected inherited eventfd or epoll anon inode, got {anon_paths:?}"
      );

      let ns_fd = reexec_fds
        .iter()
        .find(|event| fd_fstype(event) == "nsfs")
        .expect("expected inherited namespace fd");
      assert_eq!(ns_fd.uses_d_dname, 1);
      assert_eq!(fd_pseudo_name(ns_fd), "mnt");
      assert!(
        rendered_fd_path(&capture, ns_fd)
          .as_ref()
          .starts_with("mnt:["),
        "namespace fd should render as mnt:[ino]"
      );

      if let Some(pidfd) = reexec_fds.iter().find(|event| fd_fstype(event) == "pidfs") {
        assert_eq!(pidfd.uses_d_dname, 1);
        assert_eq!(
          rendered_fd_path(&capture, pidfd).as_ref(),
          "anon_inode:[pidfd]"
        );
      }

      Ok(())
    },
  )
}
