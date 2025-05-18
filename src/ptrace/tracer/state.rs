use std::{cell::RefCell, rc::Rc};

use crate::{
  cache::ArcStr,
  event::{EventId, ParentTracker},
  tracer::{ExecData, ProcessExit},
};
use chrono::{DateTime, Local};
use hashbrown::HashMap;
use nix::unistd::Pid;
use opentelemetry::{Context, trace::TraceContextExt};

use crate::{proc::read_comm, ptrace::Signal};

use super::BreakPointHit;

#[derive(Debug)]
pub struct ProcessStateStore {
  processes: HashMap<Pid, ProcessState>,
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
  pub comm: ArcStr,
  pub presyscall: bool,
  pub is_exec_successful: bool,
  pub syscall: Syscall,
  /// The timestamp of last exe
  pub timestamp: Option<DateTime<Local>>,
  pub exec_data: Option<ExecData>,
  // Two kinds of parent: replace and fork.
  pub parent_tracker: ParentTracker,
  pub associated_events: Vec<EventId>,
  /// A pending detach request with a signal to send to the process
  pub pending_detach: Option<PendingDetach>,
  /// The OTEL context for all exec event of this process
  pub otel_ctxs: Vec<Rc<RefCell<Context>>>,
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
    self.processes.entry(state.pid).or_insert(state);
  }

  pub fn get_current_mut(&mut self, pid: Pid) -> Option<&mut ProcessState> {
    // The last process in the vector is the current process
    // println!("Getting {pid}");
    self.processes.get_mut(&pid)
  }

  pub fn get_current(&self, pid: Pid) -> Option<&ProcessState> {
    // The last process in the vector is the current process
    self.processes.get(&pid)
  }

  pub fn get_current_disjoint_mut(&mut self, p1: Pid, p2: Pid) -> [Option<&mut ProcessState>; 2] {
    self.processes.get_many_mut([&p1, &p2])
  }
}

impl ProcessState {
  pub fn new(pid: Pid) -> color_eyre::Result<Self> {
    Ok(Self {
      pid,
      ppid: None,
      status: ProcessStatus::Initialized,
      comm: read_comm(pid)?,
      presyscall: true,
      is_exec_successful: false,
      syscall: Syscall::Other,
      exec_data: None,
      associated_events: Vec::new(),
      pending_detach: None,
      parent_tracker: ParentTracker::new(),
      otel_ctxs: Vec::new(),
      timestamp: None,
    })
  }

  pub fn associate_event(&mut self, id: impl IntoIterator<Item = EventId>) {
    self.associated_events.extend(id);
  }

  pub fn teriminate_otel_span(&self, timestamp: DateTime<Local>) {
    for ctx in &self.otel_ctxs {
      ctx.borrow_mut().span().end_with_timestamp(timestamp.into());
    }
  }
}
