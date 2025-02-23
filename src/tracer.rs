use enumflags2::BitFlags;
use nix::unistd::User;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
  cli::{
    args::{LogModeArgs, ModifierArgs},
    options::SeccompBpf,
  },
  event::{TracerEventDetailsKind, TracerMessage},
  printer::{Printer, PrinterArgs},
  proc::BaselineInfo,
  ptrace::InspectError,
  pty::UnixSlavePty,
};
use std::{collections::BTreeMap, sync::Arc};

use crate::{
  event::OutputMsg,
  proc::{FileDescriptorInfoCollection, Interpreter},
  ptrace::Signal,
};

#[derive(Default)]
pub struct TracerBuilder {
  pub(crate) user: Option<User>,
  pub(crate) modifier: ModifierArgs,
  pub(crate) mode: Option<TracerMode>,
  pub(crate) filter: Option<BitFlags<TracerEventDetailsKind>>,
  pub(crate) tx: Option<UnboundedSender<TracerMessage>>,
  // TODO: remove this.
  pub(crate) printer: Option<Printer>,
  pub(crate) baseline: Option<Arc<BaselineInfo>>,
  // --- ptrace specific ---
  pub(crate) seccomp_bpf: SeccompBpf,
  pub(crate) ptrace_polling_delay: Option<u64>,
}

impl TracerBuilder {
  /// Initialize a new [`TracerBuilder`]
  pub fn new() -> Self {
    Default::default()
  }

  /// Sets ptrace polling delay (in microseconds)
  ///
  /// This option is not used in eBPF tracer.
  pub fn ptrace_polling_delay(mut self, ptrace_polling_delay: Option<u64>) -> Self {
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
  pub cwd: OutputMsg,
  pub interpreters: Option<Vec<Interpreter>>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
}

impl ExecData {
  pub fn new(
    filename: OutputMsg,
    argv: Result<Vec<OutputMsg>, InspectError>,
    envp: Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>,
    cwd: OutputMsg,
    interpreters: Option<Vec<Interpreter>>,
    fdinfo: FileDescriptorInfoCollection,
  ) -> Self {
    Self {
      filename,
      argv: Arc::new(argv),
      envp: Arc::new(envp),
      cwd,
      interpreters,
      fdinfo: Arc::new(fdinfo),
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
