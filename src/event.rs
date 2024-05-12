use std::{
  borrow::Cow,
  ffi::OsStr,
  io::Write,
  path::PathBuf,
  sync::{atomic::AtomicU64, Arc},
  usize,
};

use clap::ValueEnum;
use crossterm::event::KeyEvent;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use itertools::{chain, Itertools};
use lazy_static::lazy_static;
use nix::{fcntl::OFlag, sys::signal::Signal, unistd::Pid};
use ratatui::{
  layout::Size,
  style::Styled,
  text::{Line, Span},
};
use strum::Display;
use tokio::sync::mpsc;

use crate::{
  action::CopyTarget,
  cli::args::ModifierArgs,
  printer::{escape_str_for_bash, ListPrinter},
  proc::{BaselineInfo, EnvDiff, FileDescriptorInfoCollection, Interpreter},
  tracer::{state::ProcessExit, InspectError},
  tui::theme::THEME,
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

#[derive(Debug, Clone, PartialEq)]
pub struct TracerEvent {
  pub details: TracerEventDetails,
  pub id: u64,
}

lazy_static! {
  /// A global counter for events, though it should only be used by the tracer thread.
  static ref ID: AtomicU64 = 0.into();
}

impl From<TracerEventDetails> for TracerEvent {
  fn from(details: TracerEventDetails) -> Self {
    Self {
      details,
      // TODO: Maybe we can use a weaker ordering here
      id: ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
    }
  }
}

#[derive(Debug, Clone, PartialEq, FilterableEnum)]
#[filterable_enum(kind_extra_derive=ValueEnum, kind_extra_derive=Display, kind_extra_attrs="strum(serialize_all = \"kebab-case\")")]
pub enum TracerEventDetails {
  Info(TracerMessage),
  Warning(TracerMessage),
  Error(TracerMessage),
  NewChild {
    ppid: Pid,
    pcomm: String,
    pid: Pid,
  },
  Exec(Box<ExecEvent>),
  TraceeSpawn(Pid),
  TraceeExit {
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
            Some($pid.to_string().set_style(if $result == 0 {
              THEME.pid_success
            } else if $result == (-nix::libc::ENOENT).into() {
              THEME.pid_enoent
            } else {
              THEME.pid_failure
            })),
            Some(format!("<{}>", $comm).set_style(THEME.comm)),
            Some(": ".into()),
        ], [$($t)*])
    };
}

macro_rules! tracer_exec_event_spans {
  ($pid: expr, $comm: expr, $result:expr, $($t:tt)*) => {
      chain!([
          Some($pid.to_string().set_style(if $result == 0 {
            THEME.pid_success
          } else if $result == (-nix::libc::ENOENT).into() {
            THEME.pid_enoent
          } else {
            THEME.pid_failure
          })),
          Some(if $result == 0 {
            THEME.status_process_running
          } else if $result == (-nix::libc::ENOENT).into() {
            THEME.status_exec_errno
          } else {
            THEME.status_exec_error
          }.into()),
          Some(format!("<{}>", $comm).set_style(THEME.comm)),
          Some(": ".into()),
      ], [$($t)*])
  };
}

impl TracerEventDetails {
  /// Convert the event to a TUI line
  ///
  /// This method is resource intensive and the caller should cache the result
  pub fn to_tui_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    env_in_cmdline: bool,
  ) -> Line<'static> {
    match self {
      TracerEventDetails::Info(TracerMessage { ref msg, pid }) => chain!(
        pid
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["[info]".set_style(THEME.tracer_info)],
        [": ".into(), msg.clone().set_style(THEME.tracer_info)]
      )
      .collect(),
      TracerEventDetails::Warning(TracerMessage { ref msg, pid }) => chain!(
        pid
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["[warn]".set_style(THEME.tracer_warning)],
        [": ".into(), msg.clone().set_style(THEME.tracer_warning)]
      )
      .collect(),
      TracerEventDetails::Error(TracerMessage { ref msg, pid }) => chain!(
        pid
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["error".set_style(THEME.tracer_error)],
        [": ".into(), msg.clone().set_style(THEME.tracer_error)]
      )
      .collect(),
      TracerEventDetails::NewChild { ppid, pcomm, pid } => {
        let spans = tracer_event_spans!(
          ppid,
          pcomm,
          0,
          Some("new child ".set_style(THEME.tracer_event)),
          Some(pid.to_string().set_style(THEME.new_child_pid)),
        );
        spans.flatten().collect()
      }
      TracerEventDetails::Exec(exec) => {
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
        let mut spans: Vec<Span> = if !cmdline_only {
          tracer_exec_event_spans!(
            pid,
            comm,
            *result,
            Some("env".set_style(THEME.tracer_event)),
          )
          .flatten()
          .collect()
        } else {
          vec!["env".set_style(THEME.tracer_event)]
        };
        let space: Span = " ".into();

        // Handle argv[0]
        let _ = argv.as_deref().inspect(|v| {
          v.first().inspect(|&arg0| {
            if filename.is_ok() && filename.as_ref().unwrap().as_os_str() != OsStr::new(arg0) {
              spans.push(space.clone());
              spans.push(format!("-a {}", escape_str_for_bash!(arg0)).set_style(THEME.arg0))
            }
          });
        });
        // Handle cwd
        if cwd != &baseline.cwd {
          spans.push(space.clone());
          spans.push(format!("-C {}", escape_str_for_bash!(cwd)).set_style(THEME.cwd));
        }
        if env_in_cmdline {
          if let Ok(env_diff) = env_diff {
            // Handle env diff
            for k in env_diff.removed.iter() {
              spans.push(space.clone());
              spans
                .push(format!("-u {}", escape_str_for_bash!(k)).set_style(THEME.deleted_env_var));
            }
            for (k, v) in env_diff.added.iter() {
              // Added env vars
              spans.push(space.clone());
              spans.push(
                format!("{}={}", escape_str_for_bash!(k), escape_str_for_bash!(v))
                  .set_style(THEME.added_env_var),
              );
            }
            for (k, v) in env_diff.modified.iter() {
              // Modified env vars
              spans.push(space.clone());
              spans.push(
                format!("{}={}", escape_str_for_bash!(k), escape_str_for_bash!(v))
                  .set_style(THEME.modified_env_var),
              );
            }
          }
        }
        spans.push(space.clone());
        // Filename
        match filename {
          Ok(filename) => {
            spans.push(format!("{}", escape_str_for_bash!(filename)).set_style(THEME.filename));
          }
          Err(_) => {
            spans.push("[failed to read filename]".set_style(THEME.inline_tracer_error));
          }
        }
        // Argv[1..]
        match argv.as_ref() {
          Ok(argv) => {
            for arg in argv.iter().skip(1) {
              spans.push(space.clone());
              spans.push(format!("{}", escape_str_for_bash!(arg)).set_style(THEME.argv));
            }
          }
          Err(_) => {
            spans.push(space.clone());
            spans.push("[failed to read argv]".set_style(THEME.inline_tracer_error));
          }
        }

        // Handle file descriptors
        if modifier.stdio_in_cmdline {
          let fdinfo_orig = baseline.fdinfo.stdin().unwrap();
          if let Some(fdinfo) = fdinfo.stdin() {
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // stdin will be closed
              spans.push(space.clone());
              spans.push("0>&-".set_style(THEME.cloexec_fd_in_cmdline));
            } else if fdinfo.path != fdinfo_orig.path {
              spans.push(space.clone());
              spans.push("<".set_style(THEME.modified_fd_in_cmdline));
              spans.push(
                escape_str_for_bash!(&fdinfo.path)
                  .into_owned()
                  .set_style(THEME.modified_fd_in_cmdline),
              );
            }
          } else {
            // stdin is closed
            spans.push(space.clone());
            spans.push("0>&-".set_style(THEME.removed_fd_in_cmdline));
          }
          let fdinfo_orig = baseline.fdinfo.stdout().unwrap();
          if let Some(fdinfo) = fdinfo.stdout() {
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // stdout will be closed
              spans.push(space.clone());
              spans.push("1>&-".set_style(THEME.cloexec_fd_in_cmdline));
            } else if fdinfo.path != fdinfo_orig.path {
              spans.push(space.clone());
              spans.push(">".set_style(THEME.modified_fd_in_cmdline));
              spans.push(
                escape_str_for_bash!(&fdinfo.path)
                  .into_owned()
                  .set_style(THEME.modified_fd_in_cmdline),
              )
            }
          } else {
            // stdout is closed
            spans.push(space.clone());
            spans.push("1>&-".set_style(THEME.removed_fd_in_cmdline));
          }
          let fdinfo_orig = baseline.fdinfo.stderr().unwrap();
          if let Some(fdinfo) = fdinfo.stderr() {
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // stderr will be closed
              spans.push(space.clone());
              spans.push("2>&-".set_style(THEME.cloexec_fd_in_cmdline));
            } else if fdinfo.path != fdinfo_orig.path {
              spans.push(space.clone());
              spans.push("2>".set_style(THEME.modified_fd_in_cmdline));
              spans.push(
                escape_str_for_bash!(&fdinfo.path)
                  .into_owned()
                  .set_style(THEME.modified_fd_in_cmdline),
              );
            }
          } else {
            // stderr is closed
            spans.push(space.clone());
            spans.push("2>&-".set_style(THEME.removed_fd_in_cmdline));
          }
        }

        if modifier.fd_in_cmdline {
          for (&fd, fdinfo) in fdinfo.fdinfo.iter() {
            if fd < 3 {
              continue;
            }
            if fdinfo.flags.intersects(OFlag::O_CLOEXEC) {
              // Skip fds that will be closed upon exec
              continue;
            }
            spans.push(space.clone());
            spans.push(fd.to_string().set_style(THEME.added_fd_in_cmdline));
            spans.push(">".set_style(THEME.added_fd_in_cmdline));
            spans.push(
              escape_str_for_bash!(&fdinfo.path)
                .into_owned()
                .set_style(THEME.added_fd_in_cmdline),
            )
          }
        }

        Line::default().spans(spans)
      }
      TracerEventDetails::TraceeExit { signal, exit_code } => format!(
        "tracee exit: signal: {:?}, exit_code: {}",
        signal, exit_code
      )
      .into(),
      TracerEventDetails::TraceeSpawn(pid) => format!("tracee spawned: {}", pid).into(),
    }
  }
}

impl TracerEventDetails {
  pub fn text_for_copy<'a>(
    &'a self,
    baseline: &BaselineInfo,
    target: CopyTarget,
    modifier_args: &ModifierArgs,
    env_in_cmdline: bool,
  ) -> Cow<'a, str> {
    if let CopyTarget::Line = target {
      return self
        .to_tui_line(baseline, false, modifier_args, env_in_cmdline)
        .to_string()
        .into();
    }
    // Other targets are only available for Exec events
    let TracerEventDetails::Exec(event) = self else {
      panic!("Copy target {:?} is only available for Exec events", target);
    };
    let mut modifier_args = ModifierArgs::default();
    match target {
      CopyTarget::Commandline(_) => self
        .to_tui_line(baseline, true, &modifier_args, true)
        .to_string()
        .into(),
      CopyTarget::CommandlineWithStdio(_) => {
        modifier_args.stdio_in_cmdline = true;
        self
          .to_tui_line(baseline, true, &modifier_args, true)
          .to_string()
          .into()
      }
      CopyTarget::CommandlineWithFds(_) => {
        modifier_args.fd_in_cmdline = true;
        modifier_args.stdio_in_cmdline = true;
        self
          .to_tui_line(baseline, true, &modifier_args, true)
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

impl FilterableTracerEventDetails {
  pub fn send_if_match(
    self,
    tx: &mpsc::UnboundedSender<TracerEvent>,
    filter: BitFlags<TracerEventDetailsKind>,
  ) -> color_eyre::Result<()> {
    if let Some(evt) = self.filter_and_take(filter) {
      tx.send(evt.into())?;
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

