//! Data structures for export command
use std::{error::Error, io, sync::Arc};

use crate::{
  cache::ArcStr,
  event::{EventId, TracerEvent, TracerEventDetails, TracerMessage},
  export::{Exporter, ExporterMetadata},
  proc::Cred,
};
use nix::libc::pid_t;
use serde::Serialize;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
  event::{ExecEvent, OutputMsg},
  proc::{BaselineInfo, EnvDiff, FileDescriptorInfoCollection},
};

struct JsonExporterInner {
  output: Box<dyn std::io::Write + Send + Sync + 'static>,
  stream: UnboundedReceiver<crate::event::TracerMessage>,
  meta: ExporterMetadata,
}

impl JsonExporterInner {
  fn new(
    output: Box<dyn std::io::Write + Send + Sync + 'static>,
    meta: super::ExporterMetadata,
    stream: UnboundedReceiver<crate::event::TracerMessage>,
  ) -> Self {
    Self {
      output,
      meta,
      stream,
    }
  }
}

pub struct JsonExporter(JsonExporterInner);
pub struct JsonStreamExporter(JsonExporterInner);

impl Exporter for JsonExporter {
  type Error = color_eyre::eyre::Error;

  async fn run(mut self) -> Result<i32, Self::Error> {
    let mut json = Json {
      meta: JsonMetaData::new(self.0.meta.baseline),
      events: Vec::new(),
    };
    loop {
      match self.0.stream.recv().await {
        Some(TracerMessage::Event(TracerEvent {
          details: TracerEventDetails::TraceeExit { exit_code, .. },
          ..
        })) => {
          serialize_json_to_output(&mut self.0.output, &json, self.0.meta.pretty)?;
          self.0.output.write_all(b"\n")?;
          self.0.output.flush()?;
          return Ok(exit_code);
        }
        Some(TracerMessage::Event(TracerEvent {
          details: TracerEventDetails::Exec(exec),
          id,
        })) => {
          json.events.push(JsonExecEvent::new(id, *exec));
        }
        // channel closed abnormally.
        None | Some(TracerMessage::FatalError(_)) => {
          return Ok(1);
        }
        _ => (),
      }
    }
  }

  fn new(
    output: Box<dyn io::Write + Send + Sync + 'static>,
    meta: ExporterMetadata,
    stream: UnboundedReceiver<TracerMessage>,
  ) -> Result<Self, Self::Error> {
    Ok(Self(JsonExporterInner::new(output, meta, stream)))
  }
}

impl Exporter for JsonStreamExporter {
  type Error = color_eyre::eyre::Error;

  async fn run(mut self) -> Result<i32, Self::Error> {
    serialize_json_to_output(
      &mut self.0.output,
      &JsonMetaData::new(self.0.meta.baseline),
      self.0.meta.pretty,
    )?;
    loop {
      match self.0.stream.recv().await {
        Some(TracerMessage::Event(TracerEvent {
          details: TracerEventDetails::TraceeExit { exit_code, .. },
          ..
        })) => {
          return Ok(exit_code);
        }
        Some(TracerMessage::Event(TracerEvent {
          details: TracerEventDetails::Exec(exec),
          id,
        })) => {
          let json_event = JsonExecEvent::new(id, *exec);
          serialize_json_to_output(&mut self.0.output, &json_event, self.0.meta.pretty)?;
          self.0.output.write_all(b"\n")?;
          self.0.output.flush()?;
        }
        // channel closed abnormally.
        None | Some(TracerMessage::FatalError(_)) => {
          return Ok(1);
        }
        _ => (),
      }
    }
  }

  fn new(
    output: Box<dyn io::Write + Send + Sync + 'static>,
    meta: ExporterMetadata,
    stream: UnboundedReceiver<TracerMessage>,
  ) -> Result<Self, Self::Error> {
    Ok(Self(JsonExporterInner::new(output, meta, stream)))
  }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", content = "value", rename_all = "kebab-case")]
pub enum JsonResult<T: Clone> {
  Success(T),
  Error(String),
}

impl<T: Serialize + Clone> JsonResult<T> {
  pub fn from_result(result: Result<T, impl Error>) -> Self {
    match result {
      Ok(v) => Self::Success(v),
      Err(e) => Self::Error(e.to_string()),
    }
  }
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonExecEvent {
  pub id: EventId,
  pub pid: pid_t,
  pub cwd: OutputMsg,
  pub comm_before_exec: ArcStr,
  pub result: i64,
  pub filename: OutputMsg,
  pub argv: JsonResult<Vec<OutputMsg>>,
  pub env: JsonResult<EnvDiff>,
  pub fdinfo: FileDescriptorInfoCollection,
  pub timestamp: u64,
  pub cred: JsonResult<Cred>,
}

impl JsonExecEvent {
  #[allow(clippy::boxed_local)] //
  pub fn new(id: EventId, event: ExecEvent) -> Self {
    tracing::trace!(
      "arc stat: {}, {}",
      Arc::strong_count(&event.argv),
      Arc::strong_count(&event.fdinfo)
    );
    Self {
      id,
      pid: event.pid.as_raw(),
      cwd: event.cwd,
      comm_before_exec: event.comm,
      result: event.result,
      filename: event.filename,
      argv: JsonResult::from_result(Arc::unwrap_or_clone(event.argv)),
      env: JsonResult::from_result(event.env_diff),
      cred: JsonResult::from_result(event.cred),
      fdinfo: Arc::unwrap_or_clone(event.fdinfo),
      timestamp: event
        .timestamp
        .timestamp_nanos_opt()
        .expect("tracexec does not support dates beyond 2262-04-11") as u64,
    }
  }
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonMetaData {
  /// version of tracexec that generates this json
  pub version: &'static str,
  pub generator: &'static str,
  pub baseline: Arc<BaselineInfo>,
}

impl JsonMetaData {
  pub fn new(baseline: Arc<BaselineInfo>) -> Self {
    Self {
      version: env!("CARGO_PKG_VERSION"),
      generator: env!("CARGO_CRATE_NAME"),
      baseline,
    }
  }
}

#[derive(Debug, Clone, Serialize)]
pub struct Json {
  #[serde(flatten)]
  pub meta: JsonMetaData,
  pub events: Vec<JsonExecEvent>,
}

pub fn serialize_json_to_output<W, T>(writer: W, value: &T, pretty: bool) -> serde_json::Result<()>
where
  W: io::Write,
  T: ?Sized + Serialize,
{
  if pretty {
    serde_json::ser::to_writer_pretty(writer, value)
  } else {
    serde_json::ser::to_writer(writer, value)
  }
}
