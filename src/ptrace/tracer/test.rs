use std::{env, path::PathBuf, sync::Arc};

use rstest::{fixture, rstest};
use serial_test::file_serial;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::info;
use tracing_test::traced_test;

use crate::{
  cli::{
    args::{LogModeArgs, ModifierArgs},
    options::SeccompBpf,
  },
  event::{OutputMsg, TracerEvent, TracerEventDetails, TracerMessage},
  proc::{BaselineInfo, Interpreter},
  tracer::TracerBuilder,
};

use super::{SpawnToken, Tracer, TracerMode};

#[fixture]
fn true_executable() -> PathBuf {
  env::var_os("PATH")
    .and_then(|paths| {
      env::split_paths(&paths)
        .filter_map(|dir| {
          let full_path = dir.join("true");
          if full_path.is_file() {
            Some(full_path)
          } else {
            None
          }
        })
        .next()
    })
    .expect("executable `true` not found")
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
