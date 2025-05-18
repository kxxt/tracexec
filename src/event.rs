use std::{
  borrow::Cow,
  collections::BTreeMap,
  fmt::Debug,
  sync::{Arc, atomic::AtomicU64},
};

use crate::{cache::ArcStr, timestamp::Timestamp};
use chrono::{DateTime, Local};
use clap::ValueEnum;
use crossterm::event::KeyEvent;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use nix::{errno::Errno, libc::c_int, unistd::Pid};
use ratatui::layout::Size;
use strum::Display;
use tokio::sync::mpsc;

use crate::{
  proc::{EnvDiff, FileDescriptorInfoCollection, Interpreter},
  ptrace::Signal,
  ptrace::{BreakPointHit, InspectError},
  tracer::ProcessExit,
  tui::theme::THEME,
};

mod id;
mod message;
mod parent;
mod ui;
pub use id::*;
pub use message::*;
pub use parent::*;

#[derive(Debug, Clone, Display, PartialEq, Eq)]
pub enum Event {
  ShouldQuit,
  Key(KeyEvent),
  Tracer(TracerMessage),
  Render,
  Resize(Size),
  Init,
  Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TracerMessage {
  /// A tracer event is an event that could show in the logs or event list
  Event(TracerEvent),
  /// A state update is any event that doesn't need to show in logs or having
  /// its own line in event list.
  StateUpdate(ProcessStateUpdateEvent),
  Error(Vec<Cow<'static, str>>),
  FatalError(String),
}

impl From<TracerEvent> for TracerMessage {
  fn from(event: TracerEvent) -> Self {
    Self::Event(event)
  }
}

impl From<ProcessStateUpdateEvent> for TracerMessage {
  fn from(update: ProcessStateUpdateEvent) -> Self {
    Self::StateUpdate(update)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracerEvent {
  pub details: TracerEventDetails,
  pub id: EventId,
}

/// A global counter for events, though it should only be used by the tracer thread.
static ID: AtomicU64 = AtomicU64::new(0);

impl TracerEvent {
  pub fn allocate_id() -> EventId {
    EventId::new(ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
  }
}

impl From<TracerEventDetails> for TracerEvent {
  fn from(details: TracerEventDetails) -> Self {
    Self {
      details,
      // TODO: Maybe we can use a weaker ordering here
      id: Self::allocate_id(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, FilterableEnum)]
#[filterable_enum(kind_extra_derive=ValueEnum, kind_extra_derive=Display, kind_extra_attrs="strum(serialize_all = \"kebab-case\")")]
pub enum TracerEventDetails {
  Info(TracerEventMessage),
  Warning(TracerEventMessage),
  Error(TracerEventMessage),
  NewChild {
    timestamp: Timestamp,
    ppid: Pid,
    pcomm: ArcStr,
    pid: Pid,
  },
  Exec(Box<ExecEvent>),
  TraceeSpawn {
    pid: Pid,
    timestamp: Timestamp,
  },
  TraceeExit {
    timestamp: Timestamp,
    signal: Option<Signal>,
    exit_code: i32,
  },
}

impl TracerEventDetails {
  pub fn into_event_with_id(self, id: EventId) -> TracerEvent {
    TracerEvent { details: self, id }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracerEventMessage {
  pub pid: Option<Pid>,
  pub timestamp: Option<DateTime<Local>>,
  pub msg: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecEvent {
  pub pid: Pid,
  pub cwd: OutputMsg,
  pub comm: ArcStr,
  pub filename: OutputMsg,
  pub argv: Arc<Result<Vec<OutputMsg>, InspectError>>,
  pub envp: Arc<Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>>,
  pub interpreter: Option<Vec<Interpreter>>,
  pub env_diff: Result<EnvDiff, InspectError>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
  pub result: i64,
  pub timestamp: Timestamp,
  pub parent: Option<ParentEventId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeModifier {
  pub show_env: bool,
  pub show_cwd: bool,
}

impl Default for RuntimeModifier {
  fn default() -> Self {
    Self {
      show_env: true,
      show_cwd: true,
    }
  }
}

impl TracerEventDetails {
  pub fn into_tracer_msg(self) -> TracerMessage {
    TracerMessage::Event(self.into())
  }

  pub fn timestamp(&self) -> Option<Timestamp> {
    match self {
      Self::Info(m) | Self::Warning(m) | Self::Error(m) => m.timestamp,
      Self::Exec(exec_event) => Some(exec_event.timestamp),
      Self::NewChild { timestamp, .. }
      | Self::TraceeSpawn { timestamp, .. }
      | Self::TraceeExit { timestamp, .. } => Some(*timestamp),
    }
  }
}

impl TracerEventDetails {}

impl FilterableTracerEventDetails {
  pub fn send_if_match(
    self,
    tx: &mpsc::UnboundedSender<TracerMessage>,
    filter: BitFlags<TracerEventDetailsKind>,
  ) -> color_eyre::Result<()> {
    if let Some(evt) = self.filter_and_take(filter) {
      tx.send(TracerMessage::from(TracerEvent::from(evt)))?;
    }
    Ok(())
  }
}

macro_rules! filterable_event {
    ($($t:tt)*) => {
      crate::event::FilterableTracerEventDetails::from(crate::event::TracerEventDetails::$($t)*)
    };
}

pub(crate) use filterable_event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStateUpdate {
  Exit {
    status: ProcessExit,
    timestamp: Timestamp,
  },
  BreakPointHit(BreakPointHit),
  Resumed,
  Detached {
    hid: u64,
    timestamp: Timestamp,
  },
  ResumeError {
    hit: BreakPointHit,
    error: Errno,
  },
  DetachError {
    hit: BreakPointHit,
    error: Errno,
  },
}

impl ProcessStateUpdate {
  pub fn termination_timestamp(&self) -> Option<Timestamp> {
    match self {
      Self::Exit { timestamp, .. } | Self::Detached { timestamp, .. } => Some(*timestamp),
      _ => None,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessStateUpdateEvent {
  pub update: ProcessStateUpdate,
  pub pid: Pid,
  pub ids: Vec<EventId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
  // exec status
  ExecENOENT,
  ExecFailure,
  // process status
  ProcessRunning,
  ProcessExitedNormally,
  ProcessExitedAbnormally(c_int),
  ProcessPaused,
  ProcessDetached,
  // signaled
  ProcessKilled,
  ProcessTerminated,
  ProcessInterrupted,
  ProcessSegfault,
  ProcessAborted,
  ProcessIllegalInstruction,
  ProcessSignaled(Signal),
  // internal failure
  InternalError,
}

impl From<EventStatus> for &'static str {
  fn from(value: EventStatus) -> Self {
    match value {
      EventStatus::ExecENOENT => THEME.status_indicator_exec_enoent,
      EventStatus::ExecFailure => THEME.status_indicator_exec_error,
      EventStatus::ProcessRunning => THEME.status_indicator_process_running,
      EventStatus::ProcessExitedNormally => THEME.status_indicator_process_exited_normally,
      EventStatus::ProcessExitedAbnormally(_) => THEME.status_indicator_process_exited_abnormally,
      EventStatus::ProcessKilled => THEME.status_indicator_process_killed,
      EventStatus::ProcessTerminated => THEME.status_indicator_process_terminated,
      EventStatus::ProcessInterrupted => THEME.status_indicator_process_interrupted,
      EventStatus::ProcessSegfault => THEME.status_indicator_process_segfault,
      EventStatus::ProcessAborted => THEME.status_indicator_process_aborted,
      EventStatus::ProcessIllegalInstruction => THEME.status_indicator_process_sigill,
      EventStatus::ProcessSignaled(_) => THEME.status_indicator_process_signaled,
      EventStatus::ProcessPaused => THEME.status_indicator_process_paused,
      EventStatus::ProcessDetached => THEME.status_indicator_process_detached,
      EventStatus::InternalError => THEME.status_indicator_internal_failure,
    }
  }
}
