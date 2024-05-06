use std::{path::PathBuf, sync::Arc};

use rstest::{fixture, rstest};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
  cli::args::{ModifierArgs, TracerEventArgs, TracingArgs},
  event::TracerEvent,
  proc::{BaselineInfo, Interpreter},
  tracer::Tracer,
};

use super::TracerMode;

#[fixture]
fn tracer() -> (Arc<Tracer>, UnboundedReceiver<TracerEvent>) {
  let tracer_mod = TracerMode::Cli;
  let tracing_args = TracingArgs::default();
  let modifier_args = ModifierArgs::default();
  let tracer_event_args = TracerEventArgs {
    show_all_events: true,
    ..Default::default()
  };
  let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
  let baseline = BaselineInfo::new().unwrap();

  (
    Arc::new(
      Tracer::new(
        tracer_mod,
        tracing_args,
        modifier_args,
        tracer_event_args,
        baseline,
        tx,
        None,
      )
      .unwrap(),
    ),
    rx,
  )
}

#[rstest]
#[tokio::test]
async fn tracer_emits_exec_event(tracer: (Arc<Tracer>, UnboundedReceiver<TracerEvent>)) {
  // TODO: don't assume FHS
  let (tracer, mut rx) = tracer;
  let tracer_thread = tracer.spawn(vec!["/bin/true".to_string()], None).unwrap();
  tracer_thread.join().unwrap().unwrap();
  let events = async {
    let mut events = vec![];
    while let Some(event) = rx.recv().await {
      events.push(event);
    }
    events
  }
  .await;
  for event in events {
    if let TracerEvent::Exec(exec) = event {
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
