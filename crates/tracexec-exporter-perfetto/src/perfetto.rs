use tokio::sync::mpsc::UnboundedReceiver;
use tracexec_core::{
  event::{
    TracerEvent,
    TracerEventDetails,
    TracerMessage,
  },
  export::{
    Exporter,
    ExporterMetadata,
  },
};

use crate::{
  producer::TracePacketProducer,
  recorder::PerfettoTraceRecorder,
};

pub struct PerfettoExporter {
  stream: UnboundedReceiver<tracexec_core::event::TracerMessage>,
  recorder: PerfettoTraceRecorder<Box<dyn std::io::Write + Send + Sync + 'static>>,
  meta: ExporterMetadata,
}

impl Exporter for PerfettoExporter {
  type Error = color_eyre::eyre::Error;

  fn new(
    output: Box<dyn std::io::Write + Send + Sync + 'static>,
    meta: ExporterMetadata,
    stream: UnboundedReceiver<tracexec_core::event::TracerMessage>,
  ) -> Result<Self, Self::Error> {
    Ok(Self {
      stream,
      recorder: PerfettoTraceRecorder::new(output),
      meta,
    })
  }

  #[allow(clippy::future_not_send)]
  async fn run(mut self) -> Result<i32, Self::Error> {
    let (mut producer, initial_packet) = TracePacketProducer::new(self.meta.baseline);
    self.recorder.record(initial_packet)?;
    while let Some(message) = self.stream.recv().await {
      match message {
        TracerMessage::Event(TracerEvent {
          details: TracerEventDetails::TraceeExit { exit_code, .. },
          ..
        }) => {
          self.recorder.flush()?;
          return Ok(exit_code);
        }
        TracerMessage::FatalError(_) => {
          // Terminate exporter.
          // Let the tracer thread tell the error when being joined.
          return Ok(1);
        }
        other => {
          for packet in producer.process(other)? {
            self.recorder.record(packet)?;
          }
        }
      }
    }
    // The channel shouldn't be closed at all before receiving TraceeExit event.
    self.recorder.flush()?;
    Ok(1)
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    io::{
      self,
      Write,
    },
    sync::{
      Arc,
      Mutex,
    },
  };

  use nix::{
    errno::Errno,
    unistd::Pid,
  };
  use tokio::sync::mpsc::unbounded_channel;
  use tracexec_core::{
    cli::args::ExporterArgs,
    event::{
      EventId,
      ExecEvent,
      ExecSyscall,
      OutputMsg,
      TracerEvent,
      TracerEventDetails,
      TracerMessage,
    },
    export::{
      Exporter,
      ExporterMetadata,
    },
    proc::{
      BaselineInfo,
      Cred,
      FileDescriptorInfoCollection,
      cached_str,
    },
    timestamp::ts_from_boot_ns,
  };

  use super::PerfettoExporter;

  #[derive(Clone, Default)]
  struct SharedWriter(Arc<Mutex<Vec<u8>>>);

  impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
      let mut data = self.0.lock().unwrap();
      data.extend_from_slice(buf);
      Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
      Ok(())
    }
  }

  fn make_exec_event(pid: i32, filename: &str, result: i64) -> ExecEvent {
    ExecEvent {
      syscall: ExecSyscall::Execve,
      exec_pid: Pid::from_raw(pid),
      pid: Pid::from_raw(pid),
      cwd: OutputMsg::from(cached_str("/tmp")),
      comm: cached_str("cmd"),
      filename: OutputMsg::from(cached_str(filename)),
      argv: Arc::new(Ok(vec![OutputMsg::from(cached_str(filename))])),
      envp: Arc::new(Ok(BTreeMap::new())),
      has_dash_env: false,
      cred: Ok(Cred::default()),
      interpreter: None,
      env_diff: Err(Errno::EPERM),
      fdinfo: Arc::new(FileDescriptorInfoCollection::default()),
      result,
      timestamp: ts_from_boot_ns(0),
      parent: None,
      cgroup: tracexec_core::proc::CgroupInfo::V2 {
        path: "/".to_string(),
      },
    }
  }

  #[tokio::test]
  async fn test_perfetto_exporter_run_writes_trace() {
    let baseline = Arc::new(BaselineInfo::new().unwrap());
    let meta = ExporterMetadata {
      baseline,
      exporter_args: ExporterArgs::default(),
    };
    let (tx, rx) = unbounded_channel();
    let output = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(output.clone());
    let exporter = PerfettoExporter::new(Box::new(writer), meta, rx).unwrap();
    let exec = make_exec_event(1234, "/bin/echo", 0);
    let exec_event = TracerEvent {
      id: EventId::new(1),
      details: TracerEventDetails::Exec(Box::new(exec)),
    };
    tx.send(TracerMessage::Event(exec_event)).unwrap();
    let exit_event = TracerEventDetails::TraceeExit {
      timestamp: ts_from_boot_ns(1),
      signal: None,
      exit_code: 7,
    }
    .into_event_with_id(EventId::new(2));
    tx.send(TracerMessage::Event(exit_event)).unwrap();
    drop(tx);
    let code = exporter.run().await.unwrap();
    assert_eq!(code, 7);
    assert!(!output.lock().unwrap().is_empty());
  }
}
