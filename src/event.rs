use std::{
  borrow::Cow,
  collections::BTreeMap,
  fmt::{Debug, Display},
  hash::Hash,
  io::Write,
  sync::{Arc, atomic::AtomicU64},
};

use crate::{cache::ArcStr, timestamp::Timestamp};
use chrono::{DateTime, Local};
use clap::ValueEnum;
use crossterm::event::KeyEvent;
use either::Either;
use enumflags2::BitFlags;
use filterable_enum::FilterableEnum;
use itertools::{Itertools, chain};
use nix::{errno::Errno, fcntl::OFlag, libc::c_int, unistd::Pid};
use owo_colors::OwoColorize;
use ratatui::{
  layout::Size,
  style::{Style, Styled},
  text::{Line, Span},
};
use serde::Serialize;
use strum::Display;
use tokio::sync::mpsc;

use crate::{
  action::CopyTarget,
  cli::{self, args::ModifierArgs},
  printer::ListPrinter,
  proc::{BaselineInfo, EnvDiff, FileDescriptorInfoCollection, Interpreter, cached_string},
  ptrace::Signal,
  ptrace::{BreakPointHit, InspectError},
  tracer::ProcessExit,
  tui::{
    event_line::{EventLine, Mask},
    theme::THEME,
  },
};

#[cfg(feature = "ebpf")]
use crate::bpf::BpfError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum FriendlyError {
  InspectError(Errno),
  #[cfg(feature = "ebpf")]
  Bpf(BpfError),
}

impl PartialOrd for FriendlyError {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(Ord::cmp(self, other))
  }
}

impl Ord for FriendlyError {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    match (self, other) {
      (Self::InspectError(a), Self::InspectError(b)) => (*a as i32).cmp(&(*b as i32)),
      #[cfg(feature = "ebpf")]
      (Self::Bpf(a), Self::Bpf(b)) => a.cmp(b),
      #[cfg(feature = "ebpf")]
      (Self::InspectError(_), Self::Bpf(_)) => std::cmp::Ordering::Less,
      #[cfg(feature = "ebpf")]
      (Self::Bpf(_), Self::InspectError(_)) => std::cmp::Ordering::Greater,
    }
  }
}

impl Hash for FriendlyError {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    core::mem::discriminant(self).hash(state);
    match self {
      Self::InspectError(e) => (*e as i32).hash(state),
      #[cfg(feature = "ebpf")]
      Self::Bpf(e) => e.hash(state),
    }
  }
}

#[cfg(feature = "ebpf")]
impl From<BpfError> for FriendlyError {
  fn from(value: BpfError) -> Self {
    Self::Bpf(value)
  }
}

impl From<&FriendlyError> for &'static str {
  fn from(value: &FriendlyError) -> Self {
    match value {
      FriendlyError::InspectError(_) => "[err: failed to inspect]",
      #[cfg(feature = "ebpf")]
      FriendlyError::Bpf(_) => "[err: bpf error]",
    }
  }
}

// we need to implement custom Display so Result and Either do not fit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OutputMsg {
  Ok(ArcStr),
  // Part of the message contains error
  PartialOk(ArcStr),
  Err(FriendlyError),
}

impl AsRef<str> for OutputMsg {
  fn as_ref(&self) -> &str {
    match self {
      Self::Ok(s) => s.as_ref(),
      Self::PartialOk(s) => s.as_ref(),
      Self::Err(e) => <&'static str>::from(e),
    }
  }
}

impl Serialize for OutputMsg {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    match self {
      Self::Ok(s) => s.serialize(serializer),
      Self::PartialOk(s) => s.serialize(serializer),
      Self::Err(e) => <&'static str>::from(e).serialize(serializer),
    }
  }
}

impl Display for OutputMsg {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Ok(msg) => write!(f, "{msg:?}"),
      Self::PartialOk(msg) => write!(f, "{:?}", cli::theme::THEME.inline_error.style(msg)),
      Self::Err(e) => Display::fmt(&cli::theme::THEME.inline_error.style(&e), f),
    }
  }
}

impl Display for FriendlyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", <&'static str>::from(self))
  }
}

impl From<ArcStr> for OutputMsg {
  fn from(value: ArcStr) -> Self {
    Self::Ok(value)
  }
}

impl OutputMsg {
  pub fn not_ok(&self) -> bool {
    !matches!(self, Self::Ok(_))
  }

  pub fn is_ok_and(&self, predicate: impl FnOnce(&str) -> bool) -> bool {
    match self {
      Self::Ok(s) => predicate(s),
      Self::PartialOk(_) => false,
      Self::Err(_) => false,
    }
  }

  pub fn is_err_or(&self, predicate: impl FnOnce(&str) -> bool) -> bool {
    match self {
      Self::Ok(s) => predicate(s),
      Self::PartialOk(_) => true,
      Self::Err(_) => true,
    }
  }

  /// Join two paths with a '/', preserving the semantics of [`OutputMsg`]
  pub fn join(&self, path: impl AsRef<str>) -> Self {
    let path = path.as_ref();
    match self {
      Self::Ok(s) => Self::Ok(cached_string(format!("{s}/{path}"))),
      Self::PartialOk(s) => Self::PartialOk(cached_string(format!("{s}/{path}"))),
      Self::Err(s) => Self::PartialOk(cached_string(format!("{}/{path}", <&'static str>::from(s)))),
    }
  }

  /// Escape the content for bash shell if it is not error
  pub fn tui_bash_escaped_with_style(&self, style: Style) -> Span<'static> {
    match self {
      Self::Ok(s) => {
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash).set_style(style)
      }
      Self::PartialOk(s) => {
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash)
          .set_style(style)
          .patch_style(THEME.inline_tracer_error)
      }
      Self::Err(e) => <&'static str>::from(e).set_style(THEME.inline_tracer_error),
    }
  }

  /// Escape the content for bash shell if it is not error
  pub fn cli_bash_escaped_with_style(
    &self,
    style: owo_colors::Style,
  ) -> Either<impl Display, impl Display> {
    match self {
      Self::Ok(s) => Either::Left(style.style(shell_quote::QuoteRefExt::<String>::quoted(
        s.as_str(),
        shell_quote::Bash,
      ))),
      Self::PartialOk(s) => Either::Left(cli::theme::THEME.inline_error.style(
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash),
      )),
      Self::Err(e) => Either::Right(
        cli::theme::THEME
          .inline_error
          .style(<&'static str>::from(e)),
      ),
    }
  }

  /// Escape the content for bash shell if it is not error
  pub fn bash_escaped(&self) -> Cow<'static, str> {
    match self {
      Self::Ok(s) | Self::PartialOk(s) => Cow::Owned(shell_quote::QuoteRefExt::quoted(
        s.as_str(),
        shell_quote::Bash,
      )),
      Self::Err(e) => Cow::Borrowed(<&'static str>::from(e)),
    }
  }

  pub fn tui_styled(&self, style: Style) -> Span {
    match self {
      Self::Ok(s) => (*s).set_style(style),
      Self::PartialOk(s) => (*s).set_style(THEME.inline_tracer_error),
      Self::Err(e) => <&'static str>::from(e).set_style(THEME.inline_tracer_error),
    }
  }

  pub fn cli_styled(&self, style: owo_colors::Style) -> Either<impl Display + '_, impl Display> {
    match self {
      Self::Ok(s) => Either::Left(s.style(style)),
      Self::PartialOk(s) => Either::Left(s.style(cli::theme::THEME.inline_error)),
      Self::Err(e) => Either::Right(
        cli::theme::THEME
          .inline_error
          .style(<&'static str>::from(e)),
      ),
    }
  }

  pub fn cli_escaped_styled(
    &self,
    style: owo_colors::Style,
  ) -> Either<impl Display + '_, impl Display> {
    // We (ab)use Rust's Debug feature to escape our string.
    struct DebugAsDisplay<T: Debug>(T);
    impl<T: Debug> Display for DebugAsDisplay<T> {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
      }
    }
    match self {
      Self::Ok(s) => Either::Left(style.style(DebugAsDisplay(s))),
      Self::PartialOk(s) => Either::Left(cli::theme::THEME.inline_error.style(DebugAsDisplay(s))),
      Self::Err(e) => Either::Right(
        cli::theme::THEME
          .inline_error
          .style(<&'static str>::from(e)),
      ),
    }
  }
}

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
  Event(TracerEvent),
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
  pub id: u64,
}

/// A global counter for events, though it should only be used by the tracer thread.
static ID: AtomicU64 = AtomicU64::new(0);

impl TracerEvent {
  pub fn allocate_id() -> u64 {
    ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
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

  pub fn to_tui_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    rt_modifier: RuntimeModifier,
    event_status: Option<EventStatus>,
  ) -> Line<'static> {
    self
      .to_event_line(
        baseline,
        cmdline_only,
        modifier,
        rt_modifier,
        event_status,
        false,
      )
      .line
  }

  /// Convert the event to a EventLine
  ///
  /// This method is resource intensive and the caller should cache the result
  pub fn to_event_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    rt_modifier: RuntimeModifier,
    event_status: Option<EventStatus>,
    enable_mask: bool,
  ) -> EventLine {
    let mut env_range = None;
    let mut cwd_range = None;

    let rt_modifier_effective = if enable_mask {
      // Enable all modifiers so that the mask can be toggled later
      RuntimeModifier::default()
    } else {
      rt_modifier
    };

    let ts_formatter = |ts: Timestamp| {
      if modifier.timestamp {
        let fmt = modifier.inline_timestamp_format.as_deref().unwrap();
        Some(Span::styled(
          format!("{} ", ts.format(fmt)),
          THEME.inline_timestamp,
        ))
      } else {
        None
      }
    };

    let mut line = match self {
      Self::Info(TracerEventMessage {
        msg,
        pid,
        timestamp,
      }) => chain!(
        timestamp.and_then(ts_formatter),
        pid
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["[info]".set_style(THEME.tracer_info)],
        [": ".into(), msg.clone().set_style(THEME.tracer_info)]
      )
      .collect(),
      Self::Warning(TracerEventMessage {
        msg,
        pid,
        timestamp,
      }) => chain!(
        timestamp.and_then(ts_formatter),
        pid
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["[warn]".set_style(THEME.tracer_warning)],
        [": ".into(), msg.clone().set_style(THEME.tracer_warning)]
      )
      .collect(),
      Self::Error(TracerEventMessage {
        msg,
        pid,
        timestamp,
      }) => chain!(
        timestamp.and_then(ts_formatter),
        pid
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["error".set_style(THEME.tracer_error)],
        [": ".into(), msg.clone().set_style(THEME.tracer_error)]
      )
      .collect(),
      Self::NewChild {
        ppid,
        pcomm,
        pid,
        timestamp,
      } => [
        ts_formatter(*timestamp),
        Some(ppid.to_string().set_style(THEME.pid_success)),
        event_status.map(|s| <&'static str>::from(s).into()),
        Some(format!("<{}>", pcomm).set_style(THEME.comm)),
        Some(": ".into()),
        Some("new child ".set_style(THEME.tracer_event)),
        Some(pid.to_string().set_style(THEME.new_child_pid)),
      ]
      .into_iter()
      .flatten()
      .collect(),
      Self::Exec(exec) => {
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
        let mut spans = ts_formatter(exec.timestamp).into_iter().collect_vec();
        if !cmdline_only {
          spans.extend(
            [
              Some(pid.to_string().set_style(if *result == 0 {
                THEME.pid_success
              } else if *result == (-nix::libc::ENOENT) as i64 {
                THEME.pid_enoent
              } else {
                THEME.pid_failure
              })),
              event_status.map(|s| <&'static str>::from(s).into()),
              Some(format!("<{}>", comm).set_style(THEME.comm)),
              Some(": ".into()),
              Some("env".set_style(THEME.tracer_event)),
            ]
            .into_iter()
            .flatten(),
          )
        } else {
          spans.push("env".set_style(THEME.tracer_event));
        };
        let space: Span = " ".into();

        // Handle argv[0]
        let _ = argv.as_deref().inspect(|v| {
          v.first().inspect(|&arg0| {
            if filename != arg0 {
              spans.push(space.clone());
              spans.push("-a ".set_style(THEME.arg0));
              spans.push(arg0.tui_bash_escaped_with_style(THEME.arg0));
            }
          });
        });
        // Handle cwd
        if cwd != &baseline.cwd && rt_modifier_effective.show_cwd {
          let range_start = spans.len();
          spans.push(space.clone());
          spans.push("-C ".set_style(THEME.cwd));
          spans.push(cwd.tui_bash_escaped_with_style(THEME.cwd));
          cwd_range = Some(range_start..(spans.len()))
        }
        if rt_modifier_effective.show_env {
          env_range = Some((spans.len(), 0));
          if let Ok(env_diff) = env_diff {
            // Handle env diff
            for k in env_diff.removed.iter() {
              spans.push(space.clone());
              spans.push("-u ".set_style(THEME.deleted_env_var));
              spans.push(k.tui_bash_escaped_with_style(THEME.deleted_env_var));
            }
            for (k, v) in env_diff.added.iter() {
              // Added env vars
              spans.push(space.clone());
              spans.push(k.tui_bash_escaped_with_style(THEME.added_env_var));
              spans.push("=".set_style(THEME.added_env_var));
              spans.push(v.tui_bash_escaped_with_style(THEME.added_env_var));
            }
            for (k, v) in env_diff.modified.iter() {
              // Modified env vars
              spans.push(space.clone());
              spans.push(k.tui_bash_escaped_with_style(THEME.modified_env_var));
              spans.push("=".set_style(THEME.modified_env_var));
              spans.push(v.tui_bash_escaped_with_style(THEME.modified_env_var));
            }
          }
          if let Some(r) = env_range.as_mut() {
            r.1 = spans.len();
          }
        }
        spans.push(space.clone());
        // Filename
        spans.push(filename.tui_bash_escaped_with_style(THEME.filename));
        // Argv[1..]
        match argv.as_ref() {
          Ok(argv) => {
            for arg in argv.iter().skip(1) {
              spans.push(space.clone());
              spans.push(arg.tui_bash_escaped_with_style(THEME.argv));
            }
          }
          Err(_) => {
            spans.push(space.clone());
            spans.push("[failed to read argv]".set_style(THEME.inline_tracer_error));
          }
        }

        // Handle file descriptors
        if modifier.stdio_in_cmdline {
          for fd in 0..=2 {
            self.handle_stdio_fd(fd, baseline, fdinfo, &mut spans);
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
            spans.push("<>".set_style(THEME.added_fd_in_cmdline));
            spans.push(
              fdinfo
                .path
                .tui_bash_escaped_with_style(THEME.added_fd_in_cmdline),
            )
          }
        }

        Line::default().spans(spans)
      }
      Self::TraceeExit {
        signal,
        exit_code,
        timestamp,
      } => chain!(
        ts_formatter(*timestamp),
        Some(
          format!(
            "tracee exit: signal: {:?}, exit_code: {}",
            signal, exit_code
          )
          .into()
        )
      )
      .collect(),
      Self::TraceeSpawn { pid, timestamp } => chain!(
        ts_formatter(*timestamp),
        Some(format!("tracee spawned: {}", pid).into())
      )
      .collect(),
    };
    let mut cwd_mask = None;
    let mut env_mask = None;
    if enable_mask {
      if let Some(range) = cwd_range {
        let mut mask = Mask::new(range);
        if !rt_modifier.show_cwd {
          mask.toggle(&mut line);
        }
        cwd_mask.replace(mask);
      }
      if let Some((start, end)) = env_range {
        let mut mask = Mask::new(start..end);
        if !rt_modifier.show_env {
          mask.toggle(&mut line);
        }
        env_mask.replace(mask);
      }
    }
    EventLine {
      line,
      cwd_mask,
      env_mask,
    }
  }

  fn handle_stdio_fd(
    &self,
    fd: i32,
    baseline: &BaselineInfo,
    curr: &FileDescriptorInfoCollection,
    spans: &mut Vec<Span>,
  ) {
    let (fdstr, redir) = match fd {
      0 => (" 0", "<"),
      1 => (" 1", ">"),
      2 => (" 2", "2>"),
      _ => unreachable!(),
    };

    let space: Span = " ".into();
    let fdinfo_orig = baseline.fdinfo.get(fd).unwrap();
    if let Some(fdinfo) = curr.get(fd) {
      if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
        // stdio fd will be closed
        spans.push(fdstr.set_style(THEME.cloexec_fd_in_cmdline));
        spans.push(">&-".set_style(THEME.cloexec_fd_in_cmdline));
      } else if fdinfo.not_same_file_as(fdinfo_orig) {
        spans.push(space.clone());
        spans.push(redir.set_style(THEME.modified_fd_in_cmdline));
        spans.push(
          fdinfo
            .path
            .tui_bash_escaped_with_style(THEME.modified_fd_in_cmdline),
        );
      }
    } else {
      // stdio fd is closed
      spans.push(fdstr.set_style(THEME.cloexec_fd_in_cmdline));
      spans.push(">&-".set_style(THEME.removed_fd_in_cmdline));
    }
  }
}

impl TracerEventDetails {
  pub fn text_for_copy<'a>(
    &'a self,
    baseline: &BaselineInfo,
    target: CopyTarget,
    modifier_args: &ModifierArgs,
    rt_modifier: RuntimeModifier,
  ) -> Cow<'a, str> {
    if CopyTarget::Line == target {
      return self
        .to_event_line(baseline, false, modifier_args, rt_modifier, None, false)
        .to_string()
        .into();
    }
    // Other targets are only available for Exec events
    let Self::Exec(event) = self else {
      panic!("Copy target {:?} is only available for Exec events", target);
    };
    let mut modifier_args = ModifierArgs::default();
    match target {
      CopyTarget::Commandline(_) => self
        .to_event_line(
          baseline,
          true,
          &modifier_args,
          Default::default(),
          None,
          false,
        )
        .to_string()
        .into(),
      CopyTarget::CommandlineWithStdio(_) => {
        modifier_args.stdio_in_cmdline = true;
        self
          .to_event_line(
            baseline,
            true,
            &modifier_args,
            Default::default(),
            None,
            false,
          )
          .to_string()
          .into()
      }
      CopyTarget::CommandlineWithFds(_) => {
        modifier_args.fd_in_cmdline = true;
        modifier_args.stdio_in_cmdline = true;
        self
          .to_event_line(
            baseline,
            true,
            &modifier_args,
            Default::default(),
            None,
            false,
          )
          .to_string()
          .into()
      }
      CopyTarget::Env => match event.envp.as_ref() {
        Ok(envp) => envp
          .iter()
          .map(|(k, v)| format!("{}={}", k, v))
          .join("\n")
          .into(),
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
      CopyTarget::Filename => Cow::Borrowed(event.filename.as_ref()),
      CopyTarget::SyscallResult => event.result.to_string().into(),
      CopyTarget::Line => unreachable!(),
    }
  }

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
  Exit(ProcessExit),
  BreakPointHit(BreakPointHit),
  Resumed,
  Detached { hid: u64 },
  ResumeError { hit: BreakPointHit, error: Errno },
  DetachError { hit: BreakPointHit, error: Errno },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessStateUpdateEvent {
  pub update: ProcessStateUpdate,
  pub pid: Pid,
  pub ids: Vec<u64>,
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

impl Display for EventStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let icon: &str = <&'static str>::from(*self);
    write!(f, "{} ", icon)?;
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
      ProcessExitedAbnormally(code) => write!(f, "Exited({})", code)?,
      ProcessSignaled(signal) => write!(f, "Signaled({})", signal)?,
      ProcessPaused => write!(f, "Paused due to breakpoint hit")?,
      ProcessDetached => write!(f, "Detached from tracexec")?,
      InternalError => write!(f, "An internal error occurred in tracexec")?,
    }
    Ok(())
  }
}
