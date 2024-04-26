use std::path::PathBuf;

use clap::ValueEnum;
use crossterm::event::KeyEvent;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use itertools::chain;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
  layout::Size,
  style::{Color, Stylize},
  text::Line,
};
use strum::Display;
use tokio::sync::mpsc::{self, error::SendError};

use crate::proc::Interpreter;

#[derive(Debug, Clone, Display, PartialEq)]
pub enum Event {
  ShouldQuit,
  Key(KeyEvent),
  Tracer(TracerEvent),
  Render,
  Resize(Size),
  Init,
  Error,
}

#[derive(Debug, Clone, PartialEq, FilterableEnum)]
#[filterable_enum(kind_extra_derive=ValueEnum, kind_extra_derive=Display, kind_extra_attrs="strum(serialize_all = \"kebab-case\")")]
pub enum TracerEvent {
  Info(TracerMessage),
  Warning(TracerMessage),
  Error(TracerMessage),
  NewChild {
    ppid: Pid,
    pcomm: String,
    pid: Pid,
  },
  Exec(ExecEvent),
  RootChildSpawn(Pid),
  RootChildExit {
    signal: Option<Signal>,
    exit_code: i32,
  },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracerMessage {
  pub pid: Option<Pid>,
  pub msg: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecEvent {
  pub pid: Pid,
  pub cwd: PathBuf,
  pub comm: String,
  pub filename: PathBuf,
  pub argv: Vec<String>,
  pub interpreter: Vec<Interpreter>,
  pub envp: Vec<String>,
  pub result: i64,
}


macro_rules! tracer_event_spans {
    ($pid: expr, $comm: expr, $($t:tt)*) => {
        chain!([
            Some($pid.to_string().fg(Color::Yellow)),
            Some(format!("<{}>", $comm).fg(Color::Cyan)),
            Some(": ".into()),
        ], [$($t)*])
    };
}

impl TracerEvent {
  pub fn to_tui_line(&self) -> Line {
    match self {
      TracerEvent::Info(TracerMessage { ref msg, pid }) => chain!(
        ["info".bg(Color::LightBlue)],
        pid
          .map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
          .unwrap_or_default(),
        [": ".into(), msg.as_str().into()]
      )
      .collect(),
      TracerEvent::Warning(TracerMessage { ref msg, pid }) => chain!(
        ["warn".bg(Color::Yellow)],
        pid
          .map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
          .unwrap_or_default(),
        [": ".into(), msg.as_str().into()]
      )
      .collect(),
      TracerEvent::Error(TracerMessage { ref msg, pid }) => chain!(
        ["error".bg(Color::Red)],
        pid
          .map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
          .unwrap_or_default(),
        [": ".into(), msg.as_str().into()]
      )
      .collect(),
      TracerEvent::NewChild { ppid, pcomm, pid } => {
        let spans = tracer_event_spans!(
          ppid,
          pcomm,
          Some("new child ".fg(Color::Magenta)),
          Some(pid.to_string().fg(Color::Yellow)),
        );
        spans.flatten().collect()
      }
      TracerEvent::Exec(ExecEvent {
        pid,
        cwd: _,
        comm,
        filename,
        argv,
        interpreter,
        envp,
        result,
      }) => {
        let spans = tracer_event_spans!(
          pid,
          comm,
          Some("exec ".fg(Color::Magenta)),
          Some(filename.display().to_string().fg(Color::Green)),
          Some(" argv: [".into()),
          Some(argv.join(", ").fg(Color::Green)),
          Some("]".into()),
          Some(" interpreter: [".into()),
          Some(
            interpreter
              .iter()
              .map(|x| x.to_string())
              .collect::<Vec<_>>()
              .join(", ")
              .fg(Color::Green)
          ),
          Some("]".into()),
          Some(" envp: [".into()),
          Some(envp.join(", ").fg(Color::Green)),
          Some("] result: ".into()),
          Some(result.to_string().fg(Color::Yellow)),
        );
        spans.flatten().collect()
      }
      TracerEvent::RootChildExit { signal, exit_code } => format!(
        "RootChildExit: signal: {:?}, exit_code: {}",
        signal, exit_code
      )
      .into(),
      TracerEvent::RootChildSpawn(pid) => format!("RootChildSpawn: {}", pid).into(),
    }
  }
}

impl FilterableTracerEvent {
  pub fn send_if_match(
    self,
    tx: &mpsc::UnboundedSender<TracerEvent>,
    filter: BitFlags<TracerEventKind>,
  ) -> Result<(), SendError<TracerEvent>> {
    if let Some(evt) = self.filter_and_take(filter) {
      tx.send(evt)?;
    }
    Ok(())
  }
}

macro_rules! filterable_event {
    ($($t:tt)*) => {
      crate::event::FilterableTracerEvent::from(crate::event::TracerEvent::$($t)*)
    };
}

pub(crate) use filterable_event;
