use std::{
  collections::BTreeMap,
  fmt::Debug,
  io::Write,
  sync::{Arc, atomic::AtomicU64},
};

use crate::{
  cache::ArcStr,
  printer::ListPrinter,
  proc::{Cred, CredInspectError},
  timestamp::Timestamp,
};
use chrono::{DateTime, Local};
use clap::ValueEnum;
use crossterm::event::KeyEvent;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use nix::{errno::Errno, libc::c_int, unistd::Pid};
use strum::Display;
use tokio::sync::mpsc;

use crate::{
  breakpoint::BreakPointHit,
  proc::{EnvDiff, FileDescriptorInfoCollection, Interpreter},
  tracer::ProcessExit,
  tracer::{InspectError, Signal},
};

mod id;
mod message;
mod parent;
pub use id::*;
pub use message::*;
pub use parent::*;

#[derive(Debug, Clone, Display, PartialEq, Eq)]
pub enum Event {
  ShouldQuit,
  Key(KeyEvent),
  Tracer(TracerMessage),
  Render,
  Resize { width: u16, height: u16 },
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
  /// There are env var(s) whose key starts with dash
  pub has_dash_env: bool,
  pub cred: Result<Cred, CredInspectError>,
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

impl TracerEventDetails {
  pub fn argv_to_string(argv: &Result<Vec<OutputMsg>, InspectError>) -> String {
    let Ok(argv) = argv else {
      return "[failed to read argv]".into();
    };
    let mut result =
      Vec::with_capacity(argv.iter().map(|s| s.as_ref().len() + 3).sum::<usize>() + 2);
    let list_printer = ListPrinter::new(crate::printer::ColorLevel::Less);
    list_printer.print_string_list(&mut result, argv).unwrap();
    // SAFETY: argv is printed in debug format, which is always UTF-8
    unsafe { String::from_utf8_unchecked(result) }
  }

  pub fn interpreters_to_string(interpreters: &[Interpreter]) -> String {
    let mut result = Vec::new();
    let list_printer = ListPrinter::new(crate::printer::ColorLevel::Less);
    match interpreters.len() {
      0 => {
        write!(result, "{}", Interpreter::None).unwrap();
      }
      1 => {
        write!(result, "{}", interpreters[0]).unwrap();
      }
      _ => {
        list_printer.begin(&mut result).unwrap();
        for (idx, interpreter) in interpreters.iter().enumerate() {
          if idx != 0 {
            list_printer.comma(&mut result).unwrap();
          }
          write!(result, "{interpreter}").unwrap();
        }
        list_printer.end(&mut result).unwrap();
      }
    }
    // SAFETY: interpreters is printed in debug format, which is always UTF-8
    unsafe { String::from_utf8_unchecked(result) }
  }
}

impl FilterableTracerEventDetails {
  pub fn send_if_match(
    self,
    tx: &mpsc::UnboundedSender<TracerMessage>,
    filter: BitFlags<TracerEventDetailsKind>,
  ) -> Result<(), mpsc::error::SendError<TracerMessage>> {
    if let Some(evt) = self.filter_and_take(filter) {
      tx.send(TracerMessage::from(TracerEvent::from(evt)))?;
    }
    Ok(())
  }
}

#[macro_export]
macro_rules! filterable_event {
    ($($t:tt)*) => {
      tracexec_core::event::FilterableTracerEventDetails::from(tracexec_core::event::TracerEventDetails::$($t)*)
    };
}

pub use filterable_event;

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
      EventStatus::ExecENOENT => "⚠️",
      EventStatus::ExecFailure => "❌",
      EventStatus::ProcessRunning => "🟢",
      EventStatus::ProcessExitedNormally => "😇",
      EventStatus::ProcessExitedAbnormally(_) => "😡",
      EventStatus::ProcessKilled => "😵",
      EventStatus::ProcessTerminated => "🤬",
      EventStatus::ProcessInterrupted => "🥺",
      EventStatus::ProcessSegfault => "💥",
      EventStatus::ProcessAborted => "😱",
      EventStatus::ProcessIllegalInstruction => "👿",
      EventStatus::ProcessSignaled(_) => "💀",
      EventStatus::ProcessPaused => "⏸️",
      EventStatus::ProcessDetached => "🛸",
      EventStatus::InternalError => "⛔",
    }
  }
}

impl std::fmt::Display for EventStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let icon: &str = <&'static str>::from(*self);
    write!(f, "{icon} ")?;
    use EventStatus::*;
    match self {
      ExecENOENT | ExecFailure => write!(
        f,
        "Exec failed. Further process state is not available for this event."
      )?,
      ProcessRunning => write!(f, "Running")?,
      ProcessTerminated => write!(f, "Terminated")?,
      ProcessAborted => write!(f, "Aborted")?,
      ProcessSegfault => write!(f, "Segmentation fault")?,
      ProcessIllegalInstruction => write!(f, "Illegal instruction")?,
      ProcessKilled => write!(f, "Killed")?,
      ProcessInterrupted => write!(f, "Interrupted")?,
      ProcessExitedNormally => write!(f, "Exited(0)")?,
      ProcessExitedAbnormally(code) => write!(f, "Exited({code})")?,
      ProcessSignaled(signal) => write!(f, "Signaled({signal})")?,
      ProcessPaused => write!(f, "Paused due to breakpoint hit")?,
      ProcessDetached => write!(f, "Detached from tracexec")?,
      InternalError => write!(f, "An internal error occurred in tracexec")?,
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::cache::ArcStr;
  use crate::timestamp::ts_from_boot_ns;
  use chrono::Local;
  use nix::unistd::Pid;
  use std::collections::BTreeMap;
  use std::sync::Arc;

  #[test]
  fn test_event_tracer_message_conversion() {
    let te = TracerEvent {
      details: TracerEventDetails::Info(TracerEventMessage {
        pid: Some(Pid::from_raw(1)),
        timestamp: Some(Local::now()),
        msg: "info".into(),
      }),
      id: EventId::new(0),
    };

    let tm: TracerMessage = te.clone().into();
    match tm {
      TracerMessage::Event(ev) => assert_eq!(ev, te),
      _ => panic!("Expected Event variant"),
    }
  }

  #[test]
  fn test_tracer_event_allocate_id_increments() {
    let id1 = TracerEvent::allocate_id();
    let id2 = TracerEvent::allocate_id();
    assert!(id2.into_inner() > id1.into_inner());
  }

  #[test]
  fn test_tracer_event_details_timestamp() {
    let ts = ts_from_boot_ns(100000);
    let msg = TracerEventMessage {
      pid: Some(Pid::from_raw(1)),
      timestamp: Some(Local::now()),
      msg: "msg".into(),
    };

    let info_detail = TracerEventDetails::Info(msg.clone());
    assert_eq!(info_detail.timestamp(), msg.timestamp);

    let exec_event = ExecEvent {
      pid: Pid::from_raw(2),
      cwd: OutputMsg::Ok(ArcStr::from("/")),
      comm: ArcStr::from("comm"),
      filename: OutputMsg::Ok(ArcStr::from("file")),
      argv: Arc::new(Ok(vec![])),
      envp: Arc::new(Ok(BTreeMap::new())),
      has_dash_env: false,
      cred: Ok(Default::default()),
      interpreter: None,
      env_diff: Ok(EnvDiff::empty()),
      fdinfo: Arc::new(FileDescriptorInfoCollection::default()),
      result: 0,
      timestamp: ts,
      parent: None,
    };
    let exec_detail = TracerEventDetails::Exec(Box::new(exec_event.clone()));
    assert_eq!(exec_detail.timestamp(), Some(ts));
  }

  #[test]
  fn test_argv_to_string() {
    let argv_ok = Ok(vec![
      OutputMsg::Ok(ArcStr::from("arg1")),
      OutputMsg::Ok(ArcStr::from("arg2")),
    ]);
    let argv_err: Result<Vec<OutputMsg>, InspectError> = Err(InspectError::EPERM);

    let s = TracerEventDetails::argv_to_string(&argv_ok);
    assert!(s.contains("arg1") && s.contains("arg2"));

    let s_err = TracerEventDetails::argv_to_string(&argv_err);
    assert_eq!(s_err, "[failed to read argv]");
  }

  #[test]
  fn test_interpreters_to_string() {
    let none: Vec<Interpreter> = vec![];
    let one: Vec<Interpreter> = vec![Interpreter::None];
    let many: Vec<Interpreter> = vec![Interpreter::None, Interpreter::None];

    owo_colors::control::set_should_colorize(false);

    let s_none = TracerEventDetails::interpreters_to_string(&none);
    assert_eq!(s_none, "none");

    let s_one = TracerEventDetails::interpreters_to_string(&one);
    assert_eq!(s_one, "none");

    let s_many = TracerEventDetails::interpreters_to_string(&many);
    assert!(s_many.contains("none") && s_many.contains(","));
  }

  #[test]
  fn test_process_state_update_termination_timestamp() {
    let ts = ts_from_boot_ns(1000000);
    let exit = ProcessStateUpdate::Exit {
      status: ProcessExit::Code(0),
      timestamp: ts,
    };
    let detached = ProcessStateUpdate::Detached {
      hid: 1,
      timestamp: ts,
    };
    let resumed = ProcessStateUpdate::Resumed;

    assert_eq!(exit.termination_timestamp(), Some(ts));
    assert_eq!(detached.termination_timestamp(), Some(ts));
    assert_eq!(resumed.termination_timestamp(), None);
  }

  #[test]
  fn test_event_status_display() {
    let cases = [
      (EventStatus::ExecENOENT, "⚠️ Exec failed"),
      (EventStatus::ProcessRunning, "🟢 Running"),
      (EventStatus::ProcessExitedNormally, "😇 Exited(0)"),
      (EventStatus::ProcessSegfault, "💥 Segmentation fault"),
    ];

    for (status, prefix) in cases {
      let s = format!("{}", status);
      assert!(s.starts_with(prefix.split_whitespace().next().unwrap()));
    }
  }
}
