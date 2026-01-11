use std::{
  collections::BTreeMap,
  fmt::Display,
  sync::Arc,
};

use chrono::{
  DateTime,
  Local,
};
use enumflags2::BitFlags;
use nix::{
  errno::Errno,
  libc::{
    SIGRTMIN,
    c_int,
  },
  unistd::User,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
  cli::{
    args::{
      LogModeArgs,
      ModifierArgs,
    },
    options::SeccompBpf,
  },
  event::{
    OutputMsg,
    TracerEventDetailsKind,
    TracerMessage,
  },
  printer::{
    Printer,
    PrinterArgs,
  },
  proc::{
    BaselineInfo,
    Cred,
    CredInspectError,
    FileDescriptorInfoCollection,
    Interpreter,
  },
  pty::UnixSlavePty,
};

pub type InspectError = Errno;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
  Standard(nix::sys::signal::Signal),
  Realtime(u8), // u8 is enough for Linux
}

impl Signal {
  pub fn from_raw(raw: c_int) -> Self {
    match nix::sys::signal::Signal::try_from(raw) {
      Ok(sig) => Self::Standard(sig),
      // libc might reserve some RT signals for itself.
      // But from a tracer's perspective we don't need to care about it.
      // So here no validation is done for the RT signal value.
      Err(_) => Self::Realtime(raw as u8),
    }
  }

  pub fn as_raw(self) -> i32 {
    match self {
      Self::Standard(signal) => signal as i32,
      Self::Realtime(raw) => raw as i32,
    }
  }
}

impl From<nix::sys::signal::Signal> for Signal {
  fn from(value: nix::sys::signal::Signal) -> Self {
    Self::Standard(value)
  }
}

impl Display for Signal {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Standard(signal) => signal.fmt(f),
      Self::Realtime(sig) => {
        let min = SIGRTMIN();
        let delta = *sig as i32 - min;
        match delta.signum() {
          0 => write!(f, "SIGRTMIN"),
          1 => write!(f, "SIGRTMIN+{delta}"),
          -1 => write!(f, "SIGRTMIN{delta}"),
          _ => unreachable!(),
        }
      }
    }
  }
}

#[derive(Default)]
#[non_exhaustive]
pub struct TracerBuilder {
  pub user: Option<User>,
  pub modifier: ModifierArgs,
  pub mode: Option<TracerMode>,
  pub filter: Option<BitFlags<TracerEventDetailsKind>>,
  pub tx: Option<UnboundedSender<TracerMessage>>,
  // TODO: remove this.
  pub printer: Option<Printer>,
  pub baseline: Option<Arc<BaselineInfo>>,
  // --- ptrace specific ---
  pub seccomp_bpf: SeccompBpf,
  pub ptrace_polling_delay: Option<u64>,
  pub ptrace_blocking: Option<bool>,
}

impl TracerBuilder {
  /// Initialize a new [`TracerBuilder`]
  pub fn new() -> Self {
    Default::default()
  }

  /// Use blocking waitpid calls instead of polling.
  ///
  /// This mode conflicts with ptrace polling delay option
  /// This option is not used in eBPF tracer.
  pub fn ptrace_blocking(mut self, enable: bool) -> Self {
    if self.ptrace_polling_delay.is_some() && enable {
      panic!(
        "Cannot enable blocking mode when ptrace polling delay implicitly specifys polling mode"
      );
    }
    self.ptrace_blocking = Some(enable);
    self
  }

  /// Sets ptrace polling delay (in microseconds)
  /// This options conflicts with ptrace blocking mode.
  ///
  /// This option is not used in eBPF tracer.
  pub fn ptrace_polling_delay(mut self, ptrace_polling_delay: Option<u64>) -> Self {
    if Some(true) == self.ptrace_blocking && ptrace_polling_delay.is_some() {
      panic!("Cannot set ptrace_polling_delay when operating in blocking mode")
    }
    self.ptrace_polling_delay = ptrace_polling_delay;
    self
  }

  /// Sets seccomp-bpf mode for ptrace tracer
  ///
  /// Default to auto.
  /// This option is not used in eBPF tracer.
  pub fn seccomp_bpf(mut self, seccomp_bpf: SeccompBpf) -> Self {
    self.seccomp_bpf = seccomp_bpf;
    self
  }

  /// Sets the `User` used when spawning the command.
  ///
  /// Default to current user.
  pub fn user(mut self, user: Option<User>) -> Self {
    self.user = user;
    self
  }

  pub fn modifier(mut self, modifier: ModifierArgs) -> Self {
    self.modifier = modifier;
    self
  }

  /// Sets the mode for the trace e.g. TUI or Log
  pub fn mode(mut self, mode: TracerMode) -> Self {
    self.mode = Some(mode);
    self
  }

  /// Sets a filter for wanted tracer events.
  pub fn filter(mut self, filter: BitFlags<TracerEventDetailsKind>) -> Self {
    self.filter = Some(filter);
    self
  }

  /// Passes the tx part of tracer event channel
  ///
  /// By default this is not set and tracer will not send events.
  pub fn tracer_tx(mut self, tx: UnboundedSender<TracerMessage>) -> Self {
    self.tx = Some(tx);
    self
  }

  pub fn printer(mut self, printer: Printer) -> Self {
    self.printer = Some(printer);
    self
  }

  /// Create a printer from CLI options,
  ///
  /// Requires `modifier` and `baseline` to be set before calling.
  pub fn printer_from_cli(mut self, tracing_args: &LogModeArgs) -> Self {
    self.printer = Some(Printer::new(
      PrinterArgs::from_cli(tracing_args, &self.modifier),
      self.baseline.clone().unwrap(),
    ));
    self
  }

  pub fn baseline(mut self, baseline: Arc<BaselineInfo>) -> Self {
    self.baseline = Some(baseline);
    self
  }
}

#[derive(Debug)]
pub struct ExecData {
  pub filename: OutputMsg,
  pub argv: Arc<Result<Vec<OutputMsg>, InspectError>>,
  pub envp: Arc<Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>>,
  pub has_dash_env: bool,
  pub cred: Result<Cred, CredInspectError>,
  pub cwd: OutputMsg,
  pub interpreters: Option<Vec<Interpreter>>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
  pub timestamp: DateTime<Local>,
}

impl ExecData {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    filename: OutputMsg,
    argv: Result<Vec<OutputMsg>, InspectError>,
    envp: Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>,
    has_dash_env: bool,
    cred: Result<Cred, CredInspectError>,
    cwd: OutputMsg,
    interpreters: Option<Vec<Interpreter>>,
    fdinfo: FileDescriptorInfoCollection,
    timestamp: DateTime<Local>,
  ) -> Self {
    Self {
      filename,
      argv: Arc::new(argv),
      envp: Arc::new(envp),
      has_dash_env,
      cred,
      cwd,
      interpreters,
      fdinfo: Arc::new(fdinfo),
      timestamp,
    }
  }
}

pub enum TracerMode {
  Tui(Option<UnixSlavePty>),
  Log { foreground: bool },
}

impl PartialEq for TracerMode {
  fn eq(&self, other: &Self) -> bool {
    // I think a plain match is more readable here
    #[allow(clippy::match_like_matches_macro)]
    match (self, other) {
      (Self::Log { foreground: a }, Self::Log { foreground: b }) => a == b,
      _ => false,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
  Code(i32),
  Signal(Signal),
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    sync::Arc,
  };

  use chrono::Local;
  use nix::sys::signal::Signal as NixSignal;

  use super::*;
  use crate::event::OutputMsg;

  /* ---------------- Signal ---------------- */

  #[test]
  fn signal_from_raw_standard() {
    let sig = Signal::from_raw(NixSignal::SIGINT as i32);
    assert_eq!(sig, Signal::Standard(NixSignal::SIGINT));
    assert_eq!(sig.as_raw(), NixSignal::SIGINT as i32);
  }

  #[test]
  fn signal_from_raw_realtime() {
    let raw = SIGRTMIN() + 3;
    let sig = Signal::from_raw(raw);
    assert_eq!(sig, Signal::Realtime(raw as u8));
    assert_eq!(sig.as_raw(), raw);
  }

  #[test]
  fn signal_display_standard() {
    let sig = Signal::Standard(NixSignal::SIGTERM);
    assert_eq!(sig.to_string(), "SIGTERM");
  }

  #[test]
  fn signal_display_realtime_variants() {
    let min = SIGRTMIN();

    let sig_min = Signal::Realtime(min as u8);
    assert_eq!(sig_min.to_string(), "SIGRTMIN");

    let sig_plus = Signal::Realtime((min + 2) as u8);
    assert_eq!(sig_plus.to_string(), "SIGRTMIN+2");

    let sig_minus = Signal::Realtime((min - 1) as u8);
    assert_eq!(sig_minus.to_string(), "SIGRTMIN-1");
  }

  /* ---------------- TracerBuilder ---------------- */
  #[test]
  #[should_panic(expected = "Cannot enable blocking mode")]
  fn tracer_builder_blocking_conflict_panics() {
    TracerBuilder::new()
      .ptrace_polling_delay(Some(10))
      .ptrace_blocking(true);
  }

  #[test]
  #[should_panic(expected = "Cannot set ptrace_polling_delay")]
  fn tracer_builder_polling_conflict_panics() {
    TracerBuilder::new()
      .ptrace_blocking(true)
      .ptrace_polling_delay(Some(10));
  }

  #[test]
  fn tracer_builder_chaining_works() {
    let builder = TracerBuilder::new()
      .ptrace_blocking(false)
      .ptrace_polling_delay(None)
      .seccomp_bpf(SeccompBpf::Auto);

    assert_eq!(builder.ptrace_blocking, Some(false));
    assert_eq!(builder.ptrace_polling_delay, None);
  }

  /* ---------------- ExecData ---------------- */

  #[test]
  fn exec_data_new_populates_fields() {
    let filename = OutputMsg::Ok("bin".into());
    let argv = Ok(vec![
      OutputMsg::Ok("bin".into()),
      OutputMsg::Ok("-h".into()),
    ]);

    let mut envp_map = BTreeMap::new();
    envp_map.insert(OutputMsg::Ok("A".into()), OutputMsg::Ok("B".into()));
    let envp = Ok(envp_map);

    let cwd = OutputMsg::Ok("/".into());
    let fdinfo = FileDescriptorInfoCollection::default();
    let timestamp = Local::now();

    let exec = ExecData::new(
      filename.clone(),
      argv,
      envp,
      false,
      Err(CredInspectError::Inspect),
      cwd.clone(),
      None,
      fdinfo,
      timestamp,
    );

    assert_eq!(exec.filename, filename);
    assert_eq!(exec.cwd, cwd);
    assert!(exec.argv.is_ok());
    assert!(exec.envp.is_ok());
    assert!(!exec.has_dash_env);
    assert!(exec.interpreters.is_none());
    assert!(Arc::strong_count(&exec.argv) >= 1);
    assert!(Arc::strong_count(&exec.envp) >= 1);
    assert!(Arc::strong_count(&exec.fdinfo) >= 1);
  }

  /* ---------------- ProcessExit ---------------- */

  #[test]
  fn process_exit_equality() {
    let a = ProcessExit::Code(0);
    let b = ProcessExit::Code(0);
    let c = ProcessExit::Code(1);

    assert_eq!(a, b);
    assert_ne!(a, c);

    let s1 = ProcessExit::Signal(Signal::Standard(NixSignal::SIGKILL));
    let s2 = ProcessExit::Signal(Signal::Standard(NixSignal::SIGKILL));
    let s3 = ProcessExit::Signal(Signal::Standard(NixSignal::SIGTERM));

    assert_eq!(s1, s2);
    assert_ne!(s1, s3);
  }
}
