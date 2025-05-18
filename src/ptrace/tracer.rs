use std::{
  collections::BTreeMap,
  sync::{Arc, RwLock, atomic::AtomicU32},
  time::Duration,
};

use crate::{
  otel::OtelConfig,
  tracer::{TracerBuilder, TracerMode},
};
use enumflags2::BitFlags;
use inner::TracerInner;
use nix::{
  libc::{pthread_self, pthread_setname_np},
  unistd::{Pid, User},
};
use tokio::{
  sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
  task::JoinHandle,
};
use tracing::trace;

use crate::{
  cli::args::ModifierArgs,
  event::{TracerEventDetailsKind, TracerMessage},
  printer::{Printer, PrinterOut},
  proc::BaselineInfo,
  ptrace::Signal,
};

use super::breakpoint::BreakPoint;

mod inner;
mod state;
#[cfg(test)]
mod test;

use super::BreakPointHit;

use crate::cli::options::SeccompBpf;

/// PTRACE tracer implementation.
///
/// The public api is Sync but internal implementation uses a dedicated
/// tokio blocking thread which uses !Sync data structures.
///
/// Implementation wise, The [`Tracer`]` is `!Send` once it is running.
/// However, when it has not started yet, we can move it to another thread.
/// (In `spawn` with a Send wrapper)
///
/// But from a user's perspective, [`TracerBuilder::build_ptrace`] returns a
/// [`SpawnToken`] to restrict that a tracer can only spawn once. And the user
/// can call the public API of [`Tracer`] on arbitrary thread.
/// So [`Arc<Tracer>`] should be [`Send`].
#[derive(Debug)]
pub struct Tracer {
  with_tty: bool,
  mode: TracerMode,
  printer: Printer,
  modifier_args: ModifierArgs,
  filter: BitFlags<TracerEventDetailsKind>,
  baseline: Arc<BaselineInfo>,
  seccomp_bpf: SeccompBpf,
  msg_tx: UnboundedSender<TracerMessage>,
  user: Option<User>,
  req_tx: UnboundedSender<PendingRequest>,
  delay: Duration,
  otel: OtelConfig,
}

#[derive(Debug, Clone)]
pub struct RunningTracer {
  breakpoints: Arc<RwLock<BTreeMap<u32, BreakPoint>>>,
  req_tx: UnboundedSender<PendingRequest>,
  seccomp_bpf: SeccompBpf,
}

pub struct SpawnToken {
  req_rx: UnboundedReceiver<PendingRequest>,
  /// The tx part is only used to check if this token belongs
  /// to the same [`Tracer`] where it comes from.
  req_tx: UnboundedSender<PendingRequest>,
}

impl TracerBuilder {
  pub fn build_ptrace(self) -> color_eyre::Result<(Tracer, SpawnToken)> {
    let seccomp_bpf = if self.seccomp_bpf == SeccompBpf::Auto {
      // TODO: check if the kernel supports seccomp-bpf
      // Let's just enable it for now and see if anyone complains
      if self.user.is_some() {
        // Seccomp-bpf enforces no-new-privs, so when using --user to trace set(u|g)id
        // binaries, we disable seccomp-bpf by default.
        SeccompBpf::Off
      } else {
        SeccompBpf::On
      }
    } else {
      self.seccomp_bpf
    };
    let with_tty = match self.mode.as_ref().unwrap() {
      TracerMode::Tui(tty) => tty.is_some(),
      TracerMode::Log { .. } => true,
    };
    let (req_tx, req_rx) = unbounded_channel();
    Ok((
      Tracer {
        with_tty,
        seccomp_bpf,
        msg_tx: self.tx.expect("tracer_tx is required for ptrace tracer"),
        user: self.user,
        printer: self.printer.unwrap(),
        modifier_args: self.modifier,
        filter: {
          let mut filter = self
            .filter
            .unwrap_or_else(BitFlags::<TracerEventDetailsKind>::all);
          trace!("Event filter: {:?}", filter);
          if let TracerMode::Log { .. } = self.mode.as_ref().unwrap() {
            // FIXME: In logging mode, we rely on root child exit event to exit the process
            //        with the same exit code as the root child. It is not printed in logging mode.
            //        Ideally we should use another channel to send the exit code to the main thread.
            filter |= TracerEventDetailsKind::TraceeExit;
          }
          filter
        },
        baseline: self.baseline.unwrap(),
        req_tx: req_tx.clone(),
        delay: {
          let default = if seccomp_bpf == SeccompBpf::On {
            Duration::from_micros(500)
          } else {
            Duration::from_micros(1)
          };
          self
            .ptrace_polling_delay
            .map(Duration::from_micros)
            .unwrap_or(default)
        },
        mode: self.mode.unwrap(),
        otel: self.otel,
      },
      SpawnToken { req_rx, req_tx },
    ))
  }
}

pub enum PendingRequest {
  ResumeProcess(BreakPointHit),
  DetachProcess {
    hit: BreakPointHit,
    signal: Option<Signal>,
    hid: u64,
  },
  SuspendSeccompBpf(Pid),
}

impl Tracer {
  pub fn spawn(
    self,
    args: Vec<String>,
    output: Option<Box<PrinterOut>>,
    token: SpawnToken,
  ) -> color_eyre::Result<(RunningTracer, JoinHandle<color_eyre::Result<()>>)> {
    if !self.req_tx.same_channel(&token.req_tx) {
      panic!("The spawn token used does not match the tracer")
    }
    drop(token.req_tx);
    let breakpoints = Arc::new(RwLock::new(BTreeMap::new()));
    let running_tracer = RunningTracer {
      breakpoints: breakpoints.clone(),
      req_tx: self.req_tx.clone(),
      seccomp_bpf: self.seccomp_bpf,
    };
    let tx = self.msg_tx.clone();
    let tracer_thread = tokio::task::spawn_blocking({
      move || {
        unsafe {
          let current_thread = pthread_self();
          pthread_setname_np(current_thread, c"tracer".as_ptr());
        }
        let inner = TracerInner::new(self, breakpoints, output)?;
        let result = tokio::runtime::Handle::current()
          .block_on(async move { inner.run(args, token.req_rx).await });
        if let Err(e) = &result {
          tx.send(TracerMessage::FatalError(e.to_string())).unwrap();
        }
        result
      }
    });
    Ok((running_tracer, tracer_thread))
  }
}

static BREAKPOINT_ID: AtomicU32 = AtomicU32::new(0);

/// Breakpoint management
impl RunningTracer {
  pub fn add_breakpoint(&self, breakpoint: BreakPoint) -> u32 {
    let id = BREAKPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut bs = self.breakpoints.write().unwrap();
    bs.insert(id, breakpoint);
    id
  }

  pub fn replace_breakpoint(&self, id: u32, new: BreakPoint) {
    let mut bs = self.breakpoints.write().unwrap();
    if !bs.contains_key(&id) {
      panic!("Breakpoint #{id} does not exist");
    }
    bs.insert(id, new);
  }

  pub fn set_breakpoint(&self, id: u32, activated: bool) {
    let mut bs = self.breakpoints.write().unwrap();
    if let Some(b) = bs.get_mut(&id) {
      b.activated = activated;
    }
  }

  pub fn get_breakpoints(&self) -> BTreeMap<u32, BreakPoint> {
    self.breakpoints.read().unwrap().clone()
  }

  pub fn get_breakpoint_pattern_string(&self, id: u32) -> Option<String> {
    self
      .breakpoints
      .read()
      .unwrap()
      .get(&id)
      .map(|b| b.pattern.to_editable())
  }

  pub fn remove_breakpoint(&self, index: u32) {
    self.breakpoints.write().unwrap().remove(&index);
  }

  pub fn clear_breakpoints(&self) {
    self.breakpoints.write().unwrap().clear();
  }

  pub fn request_process_detach(
    &self,
    hit: BreakPointHit,
    signal: Option<Signal>,
    hid: u64,
  ) -> color_eyre::Result<()> {
    self
      .req_tx
      .send(PendingRequest::DetachProcess { hit, signal, hid })?;
    Ok(())
  }

  pub fn request_process_resume(&self, hit: BreakPointHit) -> color_eyre::Result<()> {
    self.req_tx.send(PendingRequest::ResumeProcess(hit))?;
    Ok(())
  }

  pub fn request_suspend_seccomp_bpf(&self, pid: Pid) -> color_eyre::Result<()> {
    trace!("received request to suspend {pid}'s seccomp-bpf filter");
    self.req_tx.send(PendingRequest::SuspendSeccompBpf(pid))?;
    Ok(())
  }

  pub fn seccomp_bpf(&self) -> bool {
    self.seccomp_bpf == SeccompBpf::On
  }
}

const SENTINEL_SIGNAL: Signal = Signal::Standard(nix::sys::signal::SIGSTOP);
