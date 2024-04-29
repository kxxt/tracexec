use std::{ffi::OsStr, path::PathBuf};

use clap::ValueEnum;
use crossterm::event::KeyEvent;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use itertools::chain;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
  layout::Size,
  style::{Color, Stylize},
  text::{Line, Span},
};
use strum::Display;
use tokio::sync::mpsc::{self};

use crate::{
  printer::escape_str_for_bash,
  proc::{BaselineInfo, EnvDiff, Interpreter},
};

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
  Exec(Box<ExecEvent>),
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
  pub env_diff: EnvDiff,
  pub result: i64,
}

macro_rules! tracer_event_spans {
    ($pid: expr, $comm: expr, $result:expr, $($t:tt)*) => {
        chain!([
            Some($pid.to_string().fg(if $result == 0 {
              Color::LightYellow
            } else if $result == (-nix::libc::ENOENT).into() {
              Color::LightRed
            } else {
              Color::Red
            })),
            Some(format!("<{}>", $comm).fg(Color::Cyan)),
            Some(": ".into()),
        ], [$($t)*])
    };
}

impl TracerEvent {
  pub fn to_tui_line(&self, baseline: &BaselineInfo) -> Line {
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
          0,
          Some("new child ".fg(Color::Magenta)),
          Some(pid.to_string().fg(Color::Yellow)),
        );
        spans.flatten().collect()
      }
      TracerEvent::Exec(exec) => {
        let ExecEvent {
          pid,
          cwd,
          comm,
          filename,
          argv,
          interpreter: _,
          env_diff,
          result,
        } = exec.as_ref();
        let mut spans: Vec<Span> = tracer_event_spans!(
          pid,
          comm,
          *result,
          Some(format!("{:?} ", filename).fg(Color::LightBlue)),
          Some("env".fg(Color::Magenta)),
          // Handle argv[0]
          argv.first().and_then(|arg0| {
            if filename.file_name() != Some(OsStr::new(&arg0)) {
              Some(format!(" -a {}", escape_str_for_bash!(arg0)).fg(Color::Green))
            } else {
              None
            }
          }),
          // Handle cwd
          if cwd != &baseline.cwd {
            Some(format!(" -C {}", escape_str_for_bash!(cwd)).fg(Color::LightCyan))
          } else {
            None
          },
        )
        .flatten()
        .collect();
        spans.extend(
          env_diff
            .removed
            .iter()
            .map(|k| format!(" -u {}", escape_str_for_bash!(k)).fg(Color::LightRed)),
        );
        spans.push(
          // Option separator
          " -".into(),
        );
        spans.extend(
          // Added env vars
          env_diff.added.iter().map(|(k, v)| {
            format!(" {}={}", escape_str_for_bash!(k), escape_str_for_bash!(v)).fg(Color::Green)
          }),
        );
        spans.extend(
          // Modified env vars
          env_diff.modified.iter().map(|(k, v)| {
            format!(" {}={}", escape_str_for_bash!(k), escape_str_for_bash!(v)).fg(Color::Yellow)
          }),
        );
        // Filename
        spans.push(format!(" {}", escape_str_for_bash!(filename)).fg(Color::LightBlue));
        // Argv[1..]
        spans.extend(
          argv
            .iter()
            .skip(1)
            .map(|arg| format!(" {}", escape_str_for_bash!(arg)).into()),
        );
        Line::default().spans(spans)
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
  ) -> color_eyre::Result<()> {
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
