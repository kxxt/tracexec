use crate::{ptrace::InspectError, pty::UnixSlavePty};
use std::{collections::BTreeMap, sync::Arc};

use crate::{
  event::OutputMsg,
  proc::{FileDescriptorInfoCollection, Interpreter},
  ptrace::Signal,
};

#[derive(Debug)]
pub struct ExecData {
  pub filename: OutputMsg,
  pub argv: Arc<Result<Vec<OutputMsg>, InspectError>>,
  pub envp: Arc<Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>>,
  pub cwd: OutputMsg,
  pub interpreters: Option<Vec<Interpreter>>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
}

impl ExecData {
  pub fn new(
    filename: OutputMsg,
    argv: Result<Vec<OutputMsg>, InspectError>,
    envp: Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>,
    cwd: OutputMsg,
    interpreters: Option<Vec<Interpreter>>,
    fdinfo: FileDescriptorInfoCollection,
  ) -> Self {
    Self {
      filename,
      argv: Arc::new(argv),
      envp: Arc::new(envp),
      cwd,
      interpreters,
      fdinfo: Arc::new(fdinfo),
    }
  }
}

pub enum TracerMode {
  Tui(Option<UnixSlavePty>),
  Log { foreground: bool },
}

impl PartialEq for TracerMode {
  fn eq(&self, other: &Self) -> bool {
    // I think a plain match is more readable here
    #[allow(clippy::match_like_matches_macro)]
    match (self, other) {
      (Self::Log { foreground: a }, Self::Log { foreground: b }) => a == b,
      _ => false,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
  Code(i32),
  Signal(Signal),
}
