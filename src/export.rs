//! Data structures for export command
use std::{error::Error, sync::Arc};

use crate::{cache::ArcStr, event::EventId, proc::Cred};
use nix::libc::pid_t;
use serde::Serialize;

use crate::{
  event::{ExecEvent, OutputMsg},
  proc::{BaselineInfo, EnvDiff, FileDescriptorInfoCollection},
};

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
  pub baseline: BaselineInfo,
}

impl JsonMetaData {
  pub fn new(baseline: BaselineInfo) -> Self {
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
