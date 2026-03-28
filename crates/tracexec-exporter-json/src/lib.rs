//! Data structures for export command
use std::{
  error::Error,
  io,
  sync::Arc,
};

use nix::libc::pid_t;
use serde::Serialize;
use tokio::sync::mpsc::UnboundedReceiver;
use tracexec_core::{
  cache::ArcStr,
  event::{
    EventId,
    ExecEvent,
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
    EnvDiff,
    FileDescriptorInfoCollection,
  },
};

struct JsonExporterInner {
  output: Box<dyn std::io::Write + Send + Sync + 'static>,
  stream: UnboundedReceiver<tracexec_core::event::TracerMessage>,
  meta: ExporterMetadata,
}

impl JsonExporterInner {
  fn new(
    output: Box<dyn std::io::Write + Send + Sync + 'static>,
    meta: tracexec_core::export::ExporterMetadata,
    stream: UnboundedReceiver<tracexec_core::event::TracerMessage>,
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
          serialize_json_to_output(&mut self.0.output, &json, self.0.meta.exporter_args.pretty)?;
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
      self.0.meta.exporter_args.pretty,
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
          serialize_json_to_output(
            &mut self.0.output,
            &json_event,
            self.0.meta.exporter_args.pretty,
          )?;
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
  pub syscall: String,
  pub from_non_main_thread: bool,
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
      syscall: event.syscall.to_string(),
      from_non_main_thread: event.from_non_main_thread,
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
      CredInspectError,
      FileDescriptorInfoCollection,
      cached_str,
    },
    timestamp::ts_from_boot_ns,
  };

  use super::{
    JsonResult,
    JsonStreamExporter,
  };
  use crate::JsonExporter;

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
      from_non_main_thread: false,
      pid: Pid::from_raw(pid),
      cwd: OutputMsg::from(cached_str("/tmp")),
      comm: cached_str("cmd"),
      filename: OutputMsg::from(cached_str(filename)),
      argv: Arc::new(Ok(vec![OutputMsg::from(cached_str(filename))])),
      envp: Arc::new(Ok(BTreeMap::new())),
      has_dash_env: false,
      cred: Err(CredInspectError::Inspect),
      interpreter: None,
      env_diff: Err(Errno::EPERM),
      fdinfo: Arc::new(FileDescriptorInfoCollection::default()),
      result,
      timestamp: ts_from_boot_ns(0),
      parent: None,
    }
  }

  #[test]
  fn test_json_result_from_result() {
    let ok: JsonResult<u32> = JsonResult::from_result(Ok::<u32, io::Error>(7u32));
    assert!(matches!(ok, JsonResult::Success(7)));
    let err: JsonResult<u32> =
      JsonResult::from_result(Err(io::Error::new(io::ErrorKind::Other, "boom")));
    assert!(matches!(err, JsonResult::Error(msg) if msg.contains("boom")));
  }

  #[tokio::test]
  async fn test_json_exporter_emits_json_on_exit() {
    let baseline = Arc::new(BaselineInfo::new().unwrap());
    let meta = ExporterMetadata {
      baseline,
      exporter_args: ExporterArgs::default(),
    };
    let (tx, rx) = unbounded_channel();
    let output = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(output.clone());
    let exporter = JsonExporter::new(Box::new(writer), meta, rx).unwrap();
    let exec = make_exec_event(1234, "/bin/echo", 0);
    let exec_event = TracerEvent {
      id: EventId::new(1),
      details: TracerEventDetails::Exec(Box::new(exec)),
    };
    tx.send(TracerMessage::Event(exec_event)).unwrap();
    let exit_event = TracerEventDetails::TraceeExit {
      timestamp: ts_from_boot_ns(1),
      signal: None,
      exit_code: 0,
    }
    .into_event_with_id(EventId::new(2));
    tx.send(TracerMessage::Event(exit_event)).unwrap();
    drop(tx);
    let code = exporter.run().await.unwrap();
    assert_eq!(code, 0);
    let output = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(value["events"].as_array().unwrap().len(), 1);
    assert_eq!(
      value["generator"].as_str().unwrap(),
      env!("CARGO_CRATE_NAME")
    );
  }

  #[tokio::test]
  async fn test_json_stream_exporter_emits_meta_and_events() {
    let baseline = Arc::new(BaselineInfo::new().unwrap());
    let meta = ExporterMetadata {
      baseline,
      exporter_args: ExporterArgs::default(),
    };
    let (tx, rx) = unbounded_channel();
    let output = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedWriter(output.clone());
    let exporter = JsonStreamExporter::new(Box::new(writer), meta, rx).unwrap();
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
    let output = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    let values: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&output)
      .into_iter::<serde_json::Value>()
      .collect::<Result<_, _>>()
      .unwrap();
    assert!(values.len() >= 2);
    assert!(values[0].get("generator").is_some());
    assert!(values[1].get("id").is_some());
  }
}
