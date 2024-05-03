use std::{borrow::Cow, ffi::OsStr, io::Write, path::PathBuf, sync::Arc, usize};

use clap::ValueEnum;
use crossterm::event::KeyEvent;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use itertools::{chain, Itertools};
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
  cli::args::ModifierArgs,
  inspect::InspectError,
  printer::{escape_str_for_bash, ListPrinter},
  proc::{BaselineInfo, EnvDiff, FileDescriptorInfoCollection, Interpreter},
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
  pub filename: Result<PathBuf, InspectError>,
  pub argv: Arc<Result<Vec<String>, InspectError>>,
  pub envp: Arc<Result<Vec<String>, InspectError>>,
  pub interpreter: Vec<Interpreter>,
  pub env_diff: Result<EnvDiff, InspectError>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
  pub result: i64,
}

macro_rules! tracer_event_spans {
    ($pid: expr, $comm: expr, $result:expr, $($t:tt)*) => {
        chain!([
            Some($pid.to_string().fg(if $result == 0 {
              Color::LightGreen
            } else if $result == (-nix::libc::ENOENT).into() {
              Color::LightYellow
            } else {
              Color::LightRed
            })),
            Some(format!("<{}>", $comm).fg(Color::Cyan)),
            Some(": ".into()),
        ], [$($t)*])
    };
}

impl TracerEvent {
  /// Convert the event to a TUI line
  ///
  /// This method is resource intensive and the caller should cache the result
  pub fn to_tui_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
  ) -> Line<'static> {
    match self {
      TracerEvent::Info(TracerMessage { ref msg, pid }) => chain!(
        pid
          .map(|p| [p.to_string().fg(Color::LightMagenta)])
          .unwrap_or_default(),
        ["[info]".fg(Color::LightBlue).bold()],
        [": ".into(), msg.clone().bold().light_blue()]
      )
      .collect(),
      TracerEvent::Warning(TracerMessage { ref msg, pid }) => chain!(
        pid
          .map(|p| [p.to_string().fg(Color::LightMagenta)])
          .unwrap_or_default(),
        ["[warn]".fg(Color::Yellow).bold()],
        [": ".into(), msg.clone().bold().light_yellow()]
      )
      .collect(),
      TracerEvent::Error(TracerMessage { ref msg, pid }) => chain!(
        pid
          .map(|p| [p.to_string().fg(Color::LightMagenta)])
          .unwrap_or_default(),
        ["error".bg(Color::Red).bold()],
        [": ".into(), msg.clone().bold().light_red()]
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
          fdinfo,
          ..
        } = exec.as_ref();
        let filename_or_err = match filename {
          Ok(filename) => filename.to_string_lossy().into_owned().fg(Color::LightBlue),
          Err(e) => format!("[failed to read filename: {e}]")
            .light_red()
            .bold()
            .slow_blink(),
        };

        let mut spans: Vec<Span> = if !cmdline_only {
          tracer_event_spans!(
            pid,
            comm,
            *result,
            Some(filename_or_err),
            Some(" ".into()),
            Some("env".fg(Color::Magenta)),
          )
          .flatten()
          .collect()
        } else {
          vec!["env".fg(Color::Magenta)]
        };
        let space: Span = " ".into();

        // Handle file descriptors

        if modifier.stdio_in_cmdline {
          let fdinfo_orig = baseline.fdinfo.stdin().unwrap();
          if let Some(fdinfo) = fdinfo.stdin() {
            if fdinfo.path != fdinfo_orig.path {
              spans.push(space.clone());
              spans.push("<".light_yellow().bold());
              spans.push(
                escape_str_for_bash!(&fdinfo.path)
                  .into_owned()
                  .light_yellow()
                  .bold(),
              );
            }
          } else {
            // stdin is closed
            spans.push(space.clone());
            spans.push("0>&-".light_red().bold());
          }
          let fdinfo_orig = baseline.fdinfo.stdout().unwrap();
          if let Some(fdinfo) = fdinfo.stdout() {
            if fdinfo.path != fdinfo_orig.path {
              spans.push(space.clone());
              spans.push(">".light_yellow().bold());
              spans.push(
                escape_str_for_bash!(&fdinfo.path)
                  .into_owned()
                  .light_yellow()
                  .bold(),
              )
            }
          } else {
            // stdout is closed
            spans.push(space.clone());
            spans.push("1>&-".light_red().bold());
          }
          let fdinfo_orig = baseline.fdinfo.stderr().unwrap();
          if let Some(fdinfo) = fdinfo.stderr() {
            if fdinfo.path != fdinfo_orig.path {
              spans.push(space.clone());
              spans.push("2>".light_yellow().bold());
              spans.push(
                escape_str_for_bash!(&fdinfo.path)
                  .into_owned()
                  .light_yellow()
                  .bold(),
              );
            }
          } else {
            // stderr is closed
            spans.push(space.clone());
            spans.push("2>&-".light_red().bold());
          }
        }

        if modifier.fd_in_cmdline {
          for (&fd, fdinfo) in fdinfo.fdinfo.iter() {
            if fd < 3 {
              continue;
            }
            spans.push(space.clone());
            spans.push(fd.to_string().light_green().bold());
            spans.push(">".light_green().bold());
            spans.push(
              escape_str_for_bash!(&fdinfo.path)
                .into_owned()
                .light_green()
                .bold(),
            )
          }
        }

        // Handle argv[0]
        let _ = argv.as_deref().inspect(|v| {
          v.first().inspect(|&arg0| {
            if filename.is_ok() && filename.as_ref().unwrap().as_os_str() != OsStr::new(arg0) {
              spans.push(space.clone());
              spans.push(
                format!("-a {}", escape_str_for_bash!(arg0))
                  .fg(Color::White)
                  .italic(),
              )
            }
          });
        });
        // Handle cwd
        if cwd != &baseline.cwd {
          spans.push(space.clone());
          spans.push(format!("-C {}", escape_str_for_bash!(cwd)).fg(Color::LightCyan));
        }
        if let Ok(env_diff) = env_diff {
          // Handle env diff
          for k in env_diff.removed.iter() {
            spans.push(space.clone());
            spans.push(format!("-u {}", escape_str_for_bash!(k)).fg(Color::LightRed));
          }
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
        }
        spans.push(space.clone());
        // Filename
        match filename {
          Ok(filename) => {
            spans.push(format!("{}", escape_str_for_bash!(filename)).fg(Color::LightBlue));
          }
          Err(_) => {
            spans.push(
              "[failed to read filename]"
                .fg(Color::LightRed)
                .slow_blink()
                .underlined()
                .bold(),
            );
          }
        }
        // Argv[1..]
        match argv.as_ref() {
          Ok(argv) => {
            for arg in argv.iter().skip(1) {
              spans.push(space.clone());
              spans.push(format!("{}", escape_str_for_bash!(arg)).into());
            }
          }
          Err(_) => {
            spans.push(space.clone());
            spans.push(
              "[failed to read argv]"
                .fg(Color::LightRed)
                .slow_blink()
                .underlined()
                .bold(),
            );
          }
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
  pub fn text_for_copy<'a>(
    &'a self,
    baseline: &BaselineInfo,
    target: CopyTarget,
    modifier_args: &ModifierArgs,
  ) -> Cow<'a, str> {
    if let CopyTarget::Line = target {
      return self
        .to_tui_line(baseline, false, modifier_args)
        .to_string()
        .into();
    }
    // Other targets are only available for Exec events
    let TracerEvent::Exec(event) = self else {
      panic!("Copy target {:?} is only available for Exec events", target);
    };
    let mut modifier_args = ModifierArgs::default();
    match target {
      CopyTarget::Commandline(_) => self
        .to_tui_line(baseline, true, &modifier_args)
        .to_string()
        .into(),
      CopyTarget::CommandlineWithStdio(_) => {
        modifier_args.stdio_in_cmdline = true;
        self
          .to_tui_line(baseline, true, &modifier_args)
          .to_string()
          .into()
      }
      CopyTarget::CommandlineWithFds(_) => {
        modifier_args.fd_in_cmdline = true;
        modifier_args.stdio_in_cmdline = true;
        self
          .to_tui_line(baseline, true, &modifier_args)
          .to_string()
          .into()
      }
      CopyTarget::Env => match event.envp.as_ref() {
        Ok(envp) => envp.iter().join("\n").into(),
        Err(e) => format!("[failed to read envp: {e}]").into(),
      },
      CopyTarget::EnvDiff => {
        let Ok(env_diff) = event.env_diff.as_ref() else {
          return "[failed to read envp]".into();
        };
        let mut result = String::new();
        result.push_str("# Added:\n");
        for (k, v) in env_diff.added.iter() {
          result.push_str(&format!("{}={}\n", k, v));
        }
        result.push_str("# Modified: (original first)\n");
        for (k, v) in env_diff.modified.iter() {
          result.push_str(&format!(
            "{}={}\n{}={}\n",
            k,
            baseline.env.get(k).unwrap(),
            k,
            v
          ));
        }
        result.push_str("# Removed:\n");
        for k in env_diff.removed.iter() {
          result.push_str(&format!("{}={}\n", k, baseline.env.get(k).unwrap()));
        }
        result.into()
      }
      CopyTarget::Argv => Self::argv_to_string(&event.argv).into(),
      CopyTarget::Filename => Self::filename_to_cow(&event.filename),
      CopyTarget::SyscallResult => event.result.to_string().into(),
      CopyTarget::Line => unreachable!(),
    }
  }

  pub fn filename_to_cow(filename: &Result<PathBuf, InspectError>) -> Cow<str> {
    match filename {
      Ok(filename) => filename.to_string_lossy(),
      Err(_) => "[failed to read filename]".into(),
    }
  }

  pub fn argv_to_string(argv: &Result<Vec<String>, InspectError>) -> String {
    let Ok(argv) = argv else {
      return "[failed to read argv]".into();
    };
    let mut result = Vec::with_capacity(argv.iter().map(|s| s.len() + 3).sum::<usize>() + 2);
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
          write!(result, "{}", interpreter).unwrap();
        }
        list_printer.end(&mut result).unwrap();
      }
    }
    // SAFETY: interpreters is printed in debug format, which is always UTF-8
    unsafe { String::from_utf8_unchecked(result) }
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
