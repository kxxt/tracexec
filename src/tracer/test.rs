use std::{path::PathBuf, sync::Arc};

use rstest::{fixture, rstest};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::info;
use tracing_test::traced_test;

use crate::{
  cli::args::{LogModeArgs, ModifierArgs, TracerEventArgs},
  event::{ProcessStateUpdateEvent, TracerEvent, TracerEventDetails},
  proc::{BaselineInfo, Interpreter},
  tracer::Tracer,
};

use super::TracerMode;

#[fixture]
fn tracer(
  #[default(Default::default())] modifier_args: ModifierArgs,
) -> (
  Arc<Tracer>,
  UnboundedReceiver<TracerEvent>,
  UnboundedReceiver<ProcessStateUpdateEvent>,
) {
  let tracer_mod = TracerMode::Log;
  let tracing_args = LogModeArgs::default();
  let tracer_event_args = TracerEventArgs {
    show_all_events: true,
    ..Default::default()
  };
  let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
  let (process_tx, process_rx) = tokio::sync::mpsc::unbounded_channel();
  let baseline = BaselineInfo::new().unwrap();

  (
    Arc::new(
      Tracer::new(
        tracer_mod,
        tracing_args,
        modifier_args,
        tracer_event_args,
        baseline,
        event_tx,
        process_tx,
        None,
      )
      .unwrap(),
    ),
    event_rx,
    process_rx,
  )
}

async fn run_exe_and_collect_events(
  tracer: Arc<Tracer>,
  mut rx: UnboundedReceiver<TracerEvent>,
  argv: Vec<String>,
) -> Vec<TracerEvent> {
  let tracer_thread = tracer.spawn(argv, None).unwrap();
  tracer_thread.join().unwrap().unwrap();

  async {
    let mut events = vec![];
    while let Some(event) = rx.recv().await {
      events.push(event);
    }
    events
  }
  .await
}

#[traced_test]
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test]
async fn tracer_decodes_proc_self_exe(
  #[case] resolve_proc_self_exe: bool,
  #[with(ModifierArgs {
    resolve_proc_self_exe,
    ..Default::default()
  })]
  tracer: (
    Arc<Tracer>,
    UnboundedReceiver<TracerEvent>,
    UnboundedReceiver<ProcessStateUpdateEvent>,
  ),
) {
  // Note that /proc/self/exe is the test driver binary, not tracexec
  info!(
    "tracer_decodes_proc_self_exe test: resolve_proc_self_exe={}",
    resolve_proc_self_exe
  );
  let (tracer, rx, _) = tracer;
  let events = run_exe_and_collect_events(
    tracer,
    rx,
    vec!["/proc/self/exe".to_string(), "--help".to_string()],
  )
  .await;
  let path = std::fs::read_link("/proc/self/exe").unwrap();
  for event in events {
    if let TracerEventDetails::Exec(exec) = event.details {
      let argv = exec.argv.as_deref().unwrap();
      assert_eq!(argv, &["/proc/self/exe", "--help"]);
      let filename = exec.filename.as_deref().unwrap();
      if !resolve_proc_self_exe {
        assert_eq!(filename, &PathBuf::from("/proc/self/exe"));
      } else {
        assert_eq!(filename, &path);
      }
      return;
    }
  }
  panic!("Corresponding exec event not found")
}

#[traced_test]
#[rstest]
#[tokio::test]
async fn tracer_emits_exec_event(
  tracer: (
    Arc<Tracer>,
    UnboundedReceiver<TracerEvent>,
    UnboundedReceiver<ProcessStateUpdateEvent>,
  ),
) {
  // TODO: don't assume FHS
  let (tracer, rx, _) = tracer;
  let events = run_exe_and_collect_events(tracer, rx, vec!["/bin/true".to_string()]).await;
  for event in events {
    if let TracerEventDetails::Exec(exec) = event.details {
      let argv = exec.argv.as_deref().unwrap();
      assert_eq!(argv, &["/bin/true"]);
      let filename = exec.filename.as_deref().unwrap();
      assert_eq!(filename, &PathBuf::from("/bin/true"));
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
      // Interpreter: doesn't contain errors
      for interp in exec.interpreter.iter() {
        assert!(
          !matches!(interp, Interpreter::Error(_)),
          "error: {:?}",
          interp
        );
      }
      return;
    }
  }
  panic!("Corresponding exec event not found")
}
