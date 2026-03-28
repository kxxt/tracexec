use std::{
  env,
  ffi::CString,
  os::unix::ffi::OsStrExt,
  path::PathBuf,
  sync::Arc,
};

use nix::sys::signal::Signal as NixSignal;
use rstest::{
  fixture,
  rstest,
};
use serial_test::file_serial;
use tokio::sync::mpsc::UnboundedReceiver;
use tracexec_core::{
  cli::{
    args::{
      LogModeArgs,
      ModifierArgs,
    },
    options::SeccompBpf,
  },
  event::{
    ExecSyscall,
    OutputMsg,
    TracerEvent,
    TracerEventDetails,
    TracerMessage,
  },
  proc::{
    BaselineInfo,
    Interpreter,
  },
  pty::{
    PtySize,
    PtySystem,
    native_pty_system,
  },
  tracer::{
    Signal,
    TracerBuilder,
  },
};
use tracing::info;
use tracing_test::traced_test;

use super::{
  BuildPtraceTracer,
  SpawnToken,
  Tracer,
  TracerMode,
};

fn find_executable(name: &str) -> PathBuf {
  env::var_os("PATH")
    .and_then(|paths| {
      env::split_paths(&paths)
        .filter_map(|dir| {
          let full_path = dir.join(name);
          if full_path.is_file() {
            Some(full_path)
          } else {
            None
          }
        })
        .next()
    })
    .unwrap_or_else(|| panic!("executable `{name}` not found"))
}

#[fixture]
fn true_executable() -> PathBuf {
  find_executable("true")
}

#[fixture]
fn sh_executable() -> PathBuf {
  find_executable("sh")
}

#[fixture]
fn tracer(
  #[default(Default::default())] modifier_args: ModifierArgs,
  #[default(Default::default())] seccomp_bpf: SeccompBpf,
) -> (Tracer, UnboundedReceiver<TracerMessage>, SpawnToken) {
  let tracer_mod = TracerMode::Log { foreground: false };
  let tracing_args = LogModeArgs::default();
  let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();
  let baseline = BaselineInfo::new().unwrap();
  let (tracer, token) = TracerBuilder::new()
    .mode(tracer_mod)
    .modifier(modifier_args)
    .tracer_tx(msg_tx)
    .baseline(Arc::new(baseline))
    .printer_from_cli(&tracing_args)
    .seccomp_bpf(seccomp_bpf)
    .build_ptrace()
    .unwrap();
  (tracer, msg_rx, token)
}

async fn run_exe_and_collect_msgs(
  tracer: Tracer,
  mut rx: UnboundedReceiver<TracerMessage>,
  token: SpawnToken,
  argv: Vec<String>,
) -> Vec<TracerMessage> {
  let (_tracer, tracer_thread) = tracer.spawn(argv, None, token).unwrap();
  tracer_thread.await.unwrap().unwrap();

  async {
    let mut msgs = vec![];
    while let Some(event) = rx.recv().await {
      msgs.push(event);
    }
    msgs
  }
  .await
}

type TracerFixture = (Tracer, UnboundedReceiver<TracerMessage>, SpawnToken);

#[traced_test]
#[rstest]
#[case(true)]
#[case(false)]
#[file_serial]
#[tokio::test]
async fn tracer_decodes_proc_self_exe(
  #[case] resolve_proc_self_exe: bool,
  #[with(ModifierArgs {
    resolve_proc_self_exe,
    ..Default::default()
  })]
  tracer: TracerFixture,
) {
  // Note that /proc/self/exe is the test driver binary, not tracexec
  info!(
    "tracer_decodes_proc_self_exe test: resolve_proc_self_exe={}",
    resolve_proc_self_exe
  );
  let (tracer, rx, req_rx) = tracer;
  let events = run_exe_and_collect_msgs(
    tracer,
    rx,
    req_rx,
    vec!["/proc/self/exe".to_string(), "--help".to_string()],
  )
  .await;
  let path = std::fs::read_link("/proc/self/exe").unwrap();
  for event in events {
    if let TracerMessage::Event(TracerEvent {
      details: TracerEventDetails::Exec(exec),
      ..
    }) = event
    {
      let argv = exec.argv.as_deref().unwrap();
      assert_eq!(
        argv,
        &[
          OutputMsg::Ok("/proc/self/exe".into()),
          OutputMsg::Ok("--help".into())
        ]
      );
      let OutputMsg::Ok(filename) = exec.filename else {
        panic!("Failed to inspect filename")
      };
      if !resolve_proc_self_exe {
        assert_eq!(filename, "/proc/self/exe");
      } else {
        assert_eq!(filename, path.to_string_lossy());
      }
      return;
    }
  }
  panic!("Corresponding exec event not found")
}

#[traced_test]
#[rstest]
#[case(SeccompBpf::Auto)]
#[case(SeccompBpf::Off)]
#[file_serial]
#[tokio::test]
async fn tracer_emits_exec_event(
  #[allow(unused)]
  #[case]
  seccomp_bpf: SeccompBpf,
  #[with(Default::default(), seccomp_bpf)] tracer: TracerFixture,
  true_executable: PathBuf,
) {
  let (tracer, rx, req_rx) = tracer;
  let true_executable = true_executable.to_string_lossy().to_string();
  let events = run_exe_and_collect_msgs(tracer, rx, req_rx, vec![true_executable.clone()]).await;
  for event in events {
    if let TracerMessage::Event(TracerEvent {
      details: TracerEventDetails::Exec(exec),
      ..
    }) = event
    {
      let argv = exec.argv.as_deref().unwrap();
      assert_eq!(argv, &[OutputMsg::Ok(true_executable.as_str().into())]);
      let OutputMsg::Ok(filename) = exec.filename else {
        panic!("Failed to inspect filename")
      };
      assert_eq!(filename, true_executable);
      // The environment is not modified
      let env_diff = exec.env_diff.as_ref().unwrap();
      assert!(env_diff.added.is_empty(), "added env: {:?}", env_diff.added);
      assert!(
        env_diff.removed.is_empty(),
        "removed env: {:?}",
        env_diff.removed
      );
      assert!(
        env_diff.modified.is_empty(),
        "modified env: {:?}",
        env_diff.modified
      );
      // Successful exit
      assert_eq!(exec.result, 0);
      // CWD is the same as the baseline
      assert_eq!(exec.cwd, BaselineInfo::new().unwrap().cwd);
      // File descriptors are the same as the baseline
      assert_eq!(exec.fdinfo.as_ref(), &BaselineInfo::new().unwrap().fdinfo);
      // Comm: should be the value before exec
      assert_eq!(exec.comm, "tracer");
      // Interpreter: is some(ptrace mode supports it) and doesn't contain errors
      for interp in exec.interpreter.unwrap().iter() {
        assert!(
          !matches!(interp, Interpreter::Error(_)),
          "error: {interp:?}"
        );
      }
      return;
    }
  }
  panic!("Corresponding exec event not found")
}

#[traced_test]
#[rstest]
#[file_serial]
#[tokio::test]
async fn tracer_emits_exec_event_with_tui_enabled(true_executable: PathBuf) {
  let pty = native_pty_system()
    .openpty(PtySize::default())
    .expect("openpty failed");
  let tracexec_core::pty::PtyPair { master, slave } = pty;
  let _master = master;

  let tracer_mod = TracerMode::Tui(Some(slave));
  let tracing_args = LogModeArgs::default();
  let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();
  let baseline = BaselineInfo::new().unwrap();
  let (tracer, token) = TracerBuilder::new()
    .mode(tracer_mod)
    .modifier(Default::default())
    .tracer_tx(msg_tx)
    .baseline(Arc::new(baseline))
    .printer_from_cli(&tracing_args)
    .seccomp_bpf(SeccompBpf::Auto)
    .build_ptrace()
    .unwrap();

  let true_executable = true_executable.to_string_lossy().to_string();
  let events = run_exe_and_collect_msgs(tracer, msg_rx, token, vec![true_executable.clone()]).await;
  for event in events {
    if let TracerMessage::Event(TracerEvent {
      details: TracerEventDetails::Exec(exec),
      ..
    }) = event
    {
      let argv = exec.argv.as_deref().unwrap();
      assert_eq!(argv, &[OutputMsg::Ok(true_executable.as_str().into())]);
      let OutputMsg::Ok(filename) = exec.filename else {
        panic!("Failed to inspect filename")
      };
      assert_eq!(filename, true_executable);
      return;
    }
  }
  panic!("Corresponding exec event not found")
}

#[traced_test]
#[rstest]
#[file_serial]
#[tokio::test]
async fn tracer_reports_root_tracee_signaled(
  #[with(Default::default())] tracer: TracerFixture,
  sh_executable: PathBuf,
) {
  let (tracer, rx, req_rx) = tracer;
  let sh_executable = sh_executable.to_string_lossy().to_string();
  let events = run_exe_and_collect_msgs(
    tracer,
    rx,
    req_rx,
    vec![sh_executable, "-c".to_string(), "kill -TERM $$".to_string()],
  )
  .await;

  let mut saw_tracee_exit = false;
  for event in events {
    if let TracerMessage::Event(TracerEvent {
      details: TracerEventDetails::TraceeExit {
        signal, exit_code, ..
      },
      ..
    }) = event
    {
      assert_eq!(signal, Some(Signal::Standard(NixSignal::SIGTERM)));
      assert_eq!(exit_code, 128 + NixSignal::SIGTERM as i32);
      saw_tracee_exit = true;
    }
  }
  assert!(saw_tracee_exit, "TraceeExit event not found");
}

#[traced_test]
#[rstest]
#[file_serial]
#[tokio::test]
async fn tracer_handles_execveat_syscall(
  #[with(Default::default())] tracer: TracerFixture,
  sh_executable: PathBuf,
) {
  let (tracer, rx, req_rx) = tracer;
  let sh_executable = sh_executable.to_string_lossy().to_string();
  let events = run_exe_and_collect_msgs(
    tracer,
    rx,
    req_rx,
    vec![
      "/proc/self/exe".to_string(),
      "--ignored".to_string(),
      "ptrace_execveat_helper".to_string(),
    ],
  )
  .await;

  for event in events {
    if let TracerMessage::Event(TracerEvent {
      details: TracerEventDetails::Exec(exec),
      ..
    }) = event
    {
      let OutputMsg::Ok(filename) = exec.filename else {
        continue;
      };
      if filename != sh_executable {
        continue;
      }
      let argv = exec.argv.as_deref().unwrap();
      assert_eq!(
        argv,
        &[
          OutputMsg::Ok("sh".into()),
          OutputMsg::Ok("-c".into()),
          OutputMsg::Ok("true".into())
        ]
      );
      assert_eq!(exec.syscall, ExecSyscall::Execveat);
      return;
    }
  }
  panic!("Corresponding exec event (execveat) not found");
}

#[traced_test]
#[rstest]
#[file_serial]
#[tokio::test]
async fn tracer_marks_exec_from_non_main_thread(
  #[with(Default::default())] tracer: TracerFixture,
  sh_executable: PathBuf,
) {
  let (tracer, rx, req_rx) = tracer;
  let sh_executable = sh_executable.to_string_lossy().to_string();
  let events = run_exe_and_collect_msgs(
    tracer,
    rx,
    req_rx,
    vec![
      "/proc/self/exe".to_string(),
      "--ignored".to_string(),
      "ptrace_execveat_non_main_thread_helper".to_string(),
    ],
  )
  .await;

  for event in events {
    if let TracerMessage::Event(TracerEvent {
      details: TracerEventDetails::Exec(exec),
      ..
    }) = event
    {
      let OutputMsg::Ok(filename) = exec.filename else {
        continue;
      };
      if filename != sh_executable {
        continue;
      }
      assert_eq!(exec.syscall, ExecSyscall::Execveat);
      assert!(exec.from_non_main_thread);
      return;
    }
  }
  panic!("Corresponding exec event (execveat in non-main thread) not found");
}

#[test]
#[ignore]
#[allow(unreachable_code)]
fn ptrace_execveat_helper() {
  let sh_path = find_executable("sh");
  let sh_dir = sh_path
    .parent()
    .expect("sh has no parent directory")
    .to_path_buf();
  let sh_name = sh_path
    .file_name()
    .expect("sh has no file name")
    .to_os_string();

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

  panic!("execveat in thread failed");
}

#[test]
#[ignore]
fn ptrace_execveat_non_main_thread_helper() {
  let sh_path = find_executable("sh");
  let sh_dir = sh_path
    .parent()
    .expect("sh has no parent directory")
    .to_path_buf();
  let sh_name = sh_path
    .file_name()
    .expect("sh has no file name")
    .to_os_string();

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

    panic!("execveat in thread failed");
  });

  let _ = join.join();
  panic!("execveat from non-main thread did not replace process image");
}
