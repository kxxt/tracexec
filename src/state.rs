use std::{collections::HashMap, ffi::CString, path::PathBuf, sync::Arc};

use nix::unistd::Pid;

use crate::{
  inspect::InspectError,
  proc::{read_argv, read_comm, Interpreter},
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
  pub argv: Vec<CString>,
  pub comm: String,
  pub presyscall: bool,
  pub is_exec_successful: bool,
  pub syscall: i64,
  pub exec_data: Option<ExecData>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
  SigstopReceived,
  PtraceForkEventReceived,
  Running,
  Exited(i32),
}

#[derive(Debug)]
pub struct ExecData {
  pub filename: Result<PathBuf, InspectError>,
  pub argv: Arc<Result<Vec<String>, InspectError>>,
  pub envp: Arc<Result<Vec<String>, InspectError>>,
  pub cwd: PathBuf,
  pub interpreters: Vec<Interpreter>,
}

impl ExecData {
  pub fn new(
    filename: Result<PathBuf, InspectError>,
    argv: Result<Vec<String>, InspectError>,
    envp: Result<Vec<String>, InspectError>,
    cwd: PathBuf,
    interpreters: Vec<Interpreter>,
  ) -> Self {
    Self {
      filename,
      argv: Arc::new(argv),
      envp: Arc::new(envp),
      cwd,
      interpreters,
    }
  }
}

impl ProcessStateStore {
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
      status: ProcessStatus::Running,
      comm: read_comm(pid)?,
      argv: read_argv(pid)?,
      start_time,
      presyscall: true,
      is_exec_successful: false,
      syscall: -1,
      exec_data: None,
    })
  }
}
