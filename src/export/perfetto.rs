use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
  event::{TracerEvent, TracerEventDetails, TracerMessage},
  export::{
    Exporter, ExporterMetadata,
    perfetto::{producer::TracePacketProducer, recorder::PerfettoTraceRecorder},
  },
};

mod packet;
mod producer;
mod recorder;
mod intern;

pub struct PerfettoExporter {
  stream: UnboundedReceiver<crate::event::TracerMessage>,
  recorder: PerfettoTraceRecorder<Box<dyn std::io::Write + Send + Sync + 'static>>,
  meta: ExporterMetadata,
}

impl Exporter for PerfettoExporter {
  type Error = color_eyre::eyre::Error;

  fn new(
    output: Box<dyn std::io::Write + Send + Sync + 'static>,
    meta: ExporterMetadata,
    stream: UnboundedReceiver<crate::event::TracerMessage>,
  ) -> Result<Self, Self::Error> {
    Ok(Self {
      stream,
      recorder: PerfettoTraceRecorder::new(output),
      meta,
    })
  }

  async fn run(mut self) -> Result<i32, Self::Error> {
    let (mut producer, initial_packet) = TracePacketProducer::new();
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
