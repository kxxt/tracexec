//! Data structures for export command
use std::{error::Error, path::PathBuf, sync::Arc};

use arcstr::ArcStr;
use nix::libc::pid_t;
use serde::Serialize;

use crate::{
  event::{ExecEvent, OutputMsg},
  proc::{BaselineInfo, EnvDiff, FileDescriptorInfoCollection},
};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", content = "value", rename_all = "kebab-case")]
pub enum JsonResult<T: Serialize + Clone> {
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
  pub id: u64,
  pub pid: pid_t,
  pub cwd: PathBuf,
  pub comm_before_exec: ArcStr,
  pub result: i64,
  pub filename: JsonResult<PathBuf>,
  pub argv: JsonResult<Vec<OutputMsg>>,
  pub env: JsonResult<EnvDiff>,
  pub fdinfo: FileDescriptorInfoCollection,
}

impl JsonExecEvent {
  #[allow(clippy::boxed_local)] //
  pub fn new(id: u64, event: ExecEvent) -> Self {
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
      filename: JsonResult::from_result(event.filename),
      argv: JsonResult::from_result(Arc::unwrap_or_clone(event.argv)),
      env: JsonResult::from_result(event.env_diff),
      fdinfo: Arc::unwrap_or_clone(event.fdinfo),
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
