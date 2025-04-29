use std::collections::HashMap;

use crate::{
  cache::ArcStr,
  event::EventId,
  tracer::{ExecData, ProcessExit},
};
use nix::unistd::Pid;

use crate::{proc::read_comm, ptrace::Signal};

use super::BreakPointHit;

pub struct ProcessStateStore {
  processes: HashMap<Pid, Option<ProcessState>>,
}

#[derive(Debug)]
pub struct PendingDetach {
  pub hit: BreakPointHit,
  pub hid: u64,
  pub signal: Signal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
  Execve,
  Execveat,
  Other,
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
  pub syscall: Syscall,
  pub exec_data: Option<ExecData>,
  pub associated_events: Vec<EventId>,
  /// A pending detach request with a signal to send to the process
  pub pending_detach: Option<PendingDetach>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
  Initialized,
  SigstopReceived,
  PtraceForkEventReceived,
  Running,
  Exited(ProcessExit),
  BreakPointHit,
  Detached,
}

impl ProcessStateStore {
  #[allow(clippy::new_without_default)]
  pub fn new() -> Self {
    Self {
      processes: HashMap::new(),
    }
  }

  pub fn insert(&mut self, state: ProcessState) {
    self.processes.entry(state.pid).or_default().replace(state);
  }

  pub fn get_current_mut(&mut self, pid: Pid) -> Option<&mut ProcessState> {
    // The last process in the vector is the current process
    // println!("Getting {pid}");
    self.processes.get_mut(&pid)?.as_mut()
  }

  pub fn get_current(&self, pid: Pid) -> Option<&ProcessState> {
    // The last process in the vector is the current process
    self.processes.get(&pid)?.as_ref()
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
      syscall: Syscall::Other,
      exec_data: None,
      associated_events: Vec::new(),
      pending_detach: None,
    })
  }

  pub fn associate_event(&mut self, id: impl IntoIterator<Item = EventId>) {
    self.associated_events.extend(id);
  }
}
