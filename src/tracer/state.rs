use std::{
  collections::{BTreeMap, HashMap},
  path::PathBuf,
  sync::Arc,
};

use arcstr::ArcStr;
use nix::{sys::signal::Signal, unistd::Pid};

use crate::{
  proc::{read_comm, FileDescriptorInfoCollection, Interpreter},
  tracer::InspectError,
};

pub struct ProcessStateStore {
  processes: HashMap<Pid, Vec<ProcessState>>,
}

#[derive(Debug)]
pub struct ProcessState {
  pub pid: Pid,
  pub ppid: Option<Pid>,
  pub status: ProcessStatus,
  pub start_time: u64,
  pub comm: ArcStr,
  pub presyscall: bool,
  pub is_exec_successful: bool,
  pub syscall: i64,
  pub exec_data: Option<ExecData>,
  pub associated_events: Vec<u64>,
  /// A pending detach request with a signal to send to the process
  pub pending_detach: Option<Signal>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessExit {
  Code(i32),
  Signal(Signal),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
  Initialized,
  SigstopReceived,
  PtraceForkEventReceived,
  Running,
  Exited(ProcessExit),
  BreakPointHit,
  Detached,
}

#[derive(Debug)]
pub struct ExecData {
  pub filename: Result<PathBuf, InspectError>,
  pub argv: Arc<Result<Vec<ArcStr>, InspectError>>,
  pub envp: Arc<Result<BTreeMap<ArcStr, ArcStr>, InspectError>>,
  pub cwd: PathBuf,
  pub interpreters: Vec<Interpreter>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
}

impl ExecData {
  pub fn new(
    filename: Result<PathBuf, InspectError>,
    argv: Result<Vec<ArcStr>, InspectError>,
    envp: Result<BTreeMap<ArcStr, ArcStr>, InspectError>,
    cwd: PathBuf,
    interpreters: Vec<Interpreter>,
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

impl ProcessStateStore {
  #[allow(clippy::new_without_default)]
  pub fn new() -> Self {
    Self {
      processes: HashMap::new(),
    }
  }

  pub fn insert(&mut self, state: ProcessState) {
    self.processes.entry(state.pid).or_default().push(state);
  }

  pub fn get_current_mut(&mut self, pid: Pid) -> Option<&mut ProcessState> {
    // The last process in the vector is the current process
    // println!("Getting {pid}");
    self.processes.get_mut(&pid)?.last_mut()
  }

  pub fn get_current(&self, pid: Pid) -> Option<&ProcessState> {
    // The last process in the vector is the current process
    self.processes.get(&pid)?.last()
  }
}

impl ProcessState {
  pub fn new(pid: Pid, start_time: u64) -> color_eyre::Result<Self> {
    Ok(Self {
      pid,
      ppid: None,
      status: ProcessStatus::Initialized,
      comm: read_comm(pid)?,
      start_time,
      presyscall: true,
      is_exec_successful: false,
      syscall: -1,
      exec_data: None,
      associated_events: Vec::new(),
      pending_detach: None,
    })
  }

  pub fn associate_event(&mut self, id: impl IntoIterator<Item = u64>) {
    self.associated_events.extend(id);
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreakPointStop {
  SyscallEnter,
  SyscallExit,
}

#[derive(Debug, Clone)]
pub enum BreakPointPattern {
  /// A regular expression that matches the cmdline of the process. The cmdline is the argv
  /// concatenated with spaces without any escaping.
  ArgvRegex(regex_cursor::engines::pikevm::PikeVM),
  Filename(String),
  ExactFilename(PathBuf),
}

#[derive(Debug, Clone)]
pub enum BreakPointType {
  /// The breakpoint will be hit once and then deactivated.
  Once,
  /// The breakpoint will be hit every time it is encountered.
  Permanent,
}

#[derive(Debug, Clone)]
pub struct BreakPoint {
  pub pattern: BreakPointPattern,
  pub ty: BreakPointType,
  pub activated: bool,
  pub stop: BreakPointStop,
}
