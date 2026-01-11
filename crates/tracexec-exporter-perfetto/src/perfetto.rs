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
