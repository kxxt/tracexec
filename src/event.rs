use std::{borrow::Cow, ffi::OsStr, path::PathBuf, sync::Arc, usize};

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
  action::CopyTarget,
  printer::{escape_str_for_bash, ListPrinter},
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
  pub argv: Arc<Vec<String>>,
  pub envp: Arc<Vec<String>>,
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
  pub fn to_tui_line(&self, baseline: &BaselineInfo, cmdline_only: bool) -> Line {
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
          ..
        } = exec.as_ref();
        let mut spans: Vec<Span> = if !cmdline_only {
          tracer_event_spans!(
            pid,
            comm,
            *result,
            Some(format!("{:?}", filename).fg(Color::LightBlue)),
            Some(" ".into()),
            Some("env".fg(Color::Magenta)),
          )
          .flatten()
          .collect()
        } else {
          vec!["env".fg(Color::Magenta)]
        };
        let space: Span = " ".into();
        // Handle argv[0]
        argv.first().inspect(|&arg0| {
          if filename.file_name() != Some(OsStr::new(&arg0)) {
            spans.push(space.clone());
            spans.push(
              format!("-a {}", escape_str_for_bash!(arg0))
                .fg(Color::White)
                .italic(),
            )
          }
        });
        // Handle cwd
        if cwd != &baseline.cwd {
          spans.push(space.clone());
          spans.push(format!("-C {}", escape_str_for_bash!(cwd)).fg(Color::LightCyan));
        }
        // Handle env diff
        for k in env_diff.removed.iter() {
          spans.push(space.clone());
          spans.push(format!("-u {}", escape_str_for_bash!(k)).fg(Color::LightRed));
        }
        spans.push(
          // Option separator
          " -".into(),
        );
        for (k, v) in env_diff.added.iter() {
          // Added env vars
          spans.push(space.clone());
          spans.push(
            format!("{}={}", escape_str_for_bash!(k), escape_str_for_bash!(v)).fg(Color::Green),
          );
        }
        for (k, v) in env_diff.modified.iter() {
          // Modified env vars
          spans.push(space.clone());
          spans.push(
            format!("{}={}", escape_str_for_bash!(k), escape_str_for_bash!(v)).fg(Color::Yellow),
          );
        }
        spans.push(space.clone());
        // Filename
        spans.push(format!("{}", escape_str_for_bash!(filename)).fg(Color::LightBlue));
        // Argv[1..]
        for arg in argv.iter().skip(1) {
          spans.push(space.clone());
          spans.push(format!("{}", escape_str_for_bash!(arg)).into());
        }
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

impl TracerEvent {
  pub fn text_for_copy<'a>(&'a self, baseline: &BaselineInfo, target: CopyTarget) -> Cow<'a, str> {
    if let CopyTarget::Line = target {
      return self.to_tui_line(baseline, false).to_string().into();
    }
    // Other targets are only available for Exec events
    let TracerEvent::Exec(event) = self else {
      panic!("Copy target {:?} is only available for Exec events", target);
    };
    match target {
      CopyTarget::Commandline(_) => self.to_tui_line(baseline, true).to_string().into(),
      CopyTarget::Env => "Environment".to_string().into(),
      CopyTarget::EnvDiff => "Environment Diff".to_string().into(),
      CopyTarget::Argv => {
        let mut argv =
          Vec::with_capacity(event.argv.iter().map(|s| s.len() + 3).sum::<usize>() + 2);
        let list_printer = ListPrinter::new(crate::printer::ColorLevel::Less);
        list_printer
          .print_string_list(&mut argv, &event.argv)
          .unwrap();
        // SAFETY: argv is printed in debug format, which is always UTF-8
        unsafe { String::from_utf8_unchecked(argv) }.into()
      }
      CopyTarget::Filename => event.filename.to_string_lossy(),
      CopyTarget::SyscallResult => event.result.to_string().into(),
      CopyTarget::Line => unreachable!(),
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
