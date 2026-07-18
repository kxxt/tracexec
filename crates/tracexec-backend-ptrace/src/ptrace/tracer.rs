use std::{
  collections::BTreeMap,
  sync::{
    Arc,
    RwLock,
    atomic::{
      AtomicBool,
      AtomicU32,
      Ordering,
    },
  },
  time::Duration,
};

use color_eyre::eyre::{
  bail,
  eyre,
};
use enumflags2::BitFlags;
use nix::{
  errno::Errno,
  libc::{
    c_int,
    pthread_kill,
    pthread_self,
    pthread_setname_np,
    pthread_t,
  },
  sys::signal::{
    SaFlags,
    SigAction,
    SigSet,
    sigaction,
  },
  unistd::{
    Pid,
    User,
  },
};
use tokio::sync::mpsc::{
  UnboundedReceiver,
  UnboundedSender,
  unbounded_channel,
};
use tracexec_core::{
  breakpoint::{
    BreakPoint,
    BreakPointHit,
  },
  cli::{
    args::ModifierArgs,
    options::{
      JobControl,
      SeccompBpf,
    },
  },
  event::{
    TracerEventDetailsKind,
    TracerMessage,
  },
  printer::{
    Printer,
    PrinterOut,
  },
  proc::BaselineInfo,
  tracer::{
    Signal,
    TracerBuilder,
    TracerMode,
  },
};
use tracing::{
  debug,
  trace,
  warn,
};

use crate::ptrace::{
  job_control::{
    JobControlWakeupState,
    RESOURCE_SAMPLE_INTERVAL,
  },
  tracer::{
    inner::TracerInner,
    private::Sealed,
  },
};

mod inner;
mod state;
#[cfg(test)]
mod test;

pub struct Tracer {
  with_tty: bool,
  mode: TracerMode,
  printer: Printer,
  modifier_args: ModifierArgs,
  filter: BitFlags<TracerEventDetailsKind>,
  baseline: Arc<BaselineInfo>,
  seccomp_bpf: SeccompBpf,
  job_control: Option<JobControl>,
  msg_tx: UnboundedSender<TracerMessage>,
  user: Option<User>,
  req_tx: UnboundedSender<PendingRequest>,
  polling_interval: Option<Duration>,
  tracee_env: Option<tracexec_core::elevate::EnvVars>,
}

#[derive(Debug, Clone)]
pub struct RunningTracer {
  tid: pthread_t,
  breakpoints: Arc<RwLock<BTreeMap<u32, BreakPoint>>>,
  req_tx: UnboundedSender<PendingRequest>,
  seccomp_bpf: SeccompBpf,
  blocking: bool,
}

pub struct SpawnToken {
  req_rx: UnboundedReceiver<PendingRequest>,
  /// The tx part is only used to check if this token belongs
  /// to the same [`Tracer`] where it comes from.
  req_tx: UnboundedSender<PendingRequest>,
}

mod private {
  use tracexec_core::tracer::TracerBuilder;

  pub trait Sealed {}

  impl Sealed for TracerBuilder {}
}

pub trait BuildPtraceTracer: Sealed {
  fn build_ptrace(self) -> color_eyre::Result<(Tracer, SpawnToken)>;
}

impl BuildPtraceTracer for TracerBuilder {
  fn build_ptrace(self) -> color_eyre::Result<(Tracer, SpawnToken)> {
    let mode = self.mode.ok_or_else(|| eyre!("tracer mode is required"))?;
    let msg_tx = self
      .tx
      .ok_or_else(|| eyre!("tracer event sender is required"))?;
    let printer = self.printer.ok_or_else(|| eyre!("printer is required"))?;
    let baseline = self
      .baseline
      .ok_or_else(|| eyre!("baseline process information is required"))?;
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
    let with_tty = match &mode {
      TracerMode::Tui(tty) => tty.is_some(),
      TracerMode::Log { .. } => true,
    };
    let job_control_enabled = self.job_control.is_some();
    let (req_tx, req_rx) = unbounded_channel();
    Ok((
      Tracer {
        with_tty,
        seccomp_bpf,
        job_control: self.job_control,
        msg_tx,
        user: self.user,
        printer,
        modifier_args: self.modifier,
        filter: {
          let mut filter = self
            .filter
            .unwrap_or_else(BitFlags::<TracerEventDetailsKind>::all);
          trace!("Event filter: {:?}", filter);
          if let TracerMode::Log { .. } = &mode {
            // FIXME: In logging mode, we rely on root child exit event to exit the process
            //        with the same exit code as the root child. It is not printed in logging mode.
            //        Ideally we should use another channel to send the exit code to the main thread.
            filter |= TracerEventDetailsKind::TraceeExit;
          }
          filter
        },
        baseline,
        req_tx: req_tx.clone(),
        polling_interval: {
          if self.ptrace_blocking == Some(true)
            || (job_control_enabled && self.ptrace_blocking.is_none())
          {
            None
          } else {
            let default = if seccomp_bpf == SeccompBpf::On {
              Duration::from_micros(500)
            } else {
              Duration::from_micros(1)
            };
            Some(
              self
                .ptrace_polling_delay
                .map(Duration::from_micros)
                .unwrap_or(default),
            )
          }
        },
        tracee_env: self.tracee_env,
        mode,
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
  TerminateTracer,
}

extern "C" fn empty_sighandler(_arg: c_int) {}

fn notify_tracer_thread(tid: pthread_t) -> Result<(), Errno> {
  let result = unsafe { pthread_kill(tid, nix::sys::signal::SIGUSR1 as c_int) };
  if result == 0 {
    Ok(())
  } else {
    Err(Errno::from_raw(result))
  }
}

struct JobControlWakeup {
  stop: Arc<AtomicBool>,
  thread: Option<std::thread::JoinHandle<()>>,
}

impl JobControlWakeup {
  fn spawn(
    tracer_tid: pthread_t,
    wakeup_state: Arc<JobControlWakeupState>,
    blocking_waitpid: Arc<AtomicBool>,
  ) -> std::io::Result<Self> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread_wakeup_state = wakeup_state.clone();
    let thread = std::thread::Builder::new()
      .name("jc-wakeup".to_string())
      .spawn(move || {
        loop {
          while !thread_wakeup_state.has_waiting_jobs() {
            std::thread::park();
            if thread_stop.load(Ordering::Acquire) {
              return;
            }
          }
          std::thread::park_timeout(RESOURCE_SAMPLE_INTERVAL);
          if thread_stop.load(Ordering::Acquire) {
            break;
          }
          if !thread_wakeup_state.has_waiting_jobs() || !blocking_waitpid.load(Ordering::Acquire) {
            continue;
          }
          if let Err(error) = notify_tracer_thread(tracer_tid) {
            debug!(%error, "job-control resource wakeup thread is stopping");
            break;
          }
        }
      })?;
    wakeup_state.register_worker(thread.thread().clone());
    Ok(Self {
      stop,
      thread: Some(thread),
    })
  }
}

impl Drop for JobControlWakeup {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Release);
    if let Some(thread) = self.thread.take() {
      thread.thread().unpark();
      if thread.join().is_err() {
        warn!("job-control resource wakeup thread panicked");
      }
    }
  }
}

impl Tracer {
  pub fn spawn(
    self,
    args: Vec<String>,
    output: Option<Box<PrinterOut>>,
    token: SpawnToken,
  ) -> color_eyre::Result<(
    RunningTracer,
    tokio::task::JoinHandle<color_eyre::Result<()>>,
  )> {
    if !self.req_tx.same_channel(&token.req_tx) {
      bail!("the spawn token does not belong to this tracer");
    }
    drop(token.req_tx);
    let breakpoints = Arc::new(RwLock::new(BTreeMap::new()));
    let breakpoints_clone = breakpoints.clone();
    let seccomp_bpf = self.seccomp_bpf;
    let req_tx = self.req_tx.clone();
    let blocking = self.blocking();
    let job_control_enabled = self.job_control.is_some();
    let tx = self.msg_tx.clone();
    let wakeup_state = Arc::new(JobControlWakeupState::default());
    let blocking_waitpid = Arc::new(AtomicBool::new(false));
    let (tid_tx, tid_rx) = std::sync::mpsc::sync_channel(1);
    let tracer_thread = tokio::task::spawn_blocking({
      move || {
        let current_thread = unsafe { pthread_self() };
        tid_tx.send(current_thread)?;
        unsafe {
          pthread_setname_np(current_thread, c"tracer".as_ptr());
        }
        if self.blocking() {
          // setup empty signal handler for breaking out of waitpid
          // we do not set SA_RESTART so interrupted syscalls are not restarted.
          unsafe {
            let _ = sigaction(
              nix::sys::signal::SIGUSR1,
              &SigAction::new(
                nix::sys::signal::SigHandler::Handler(empty_sighandler),
                SaFlags::empty(),
                SigSet::empty(),
              ),
            )?;
          }
        }
        let _job_control_wakeup = (blocking && job_control_enabled)
          .then(|| {
            JobControlWakeup::spawn(
              current_thread,
              wakeup_state.clone(),
              blocking_waitpid.clone(),
            )
          })
          .transpose()?;
        let inner = TracerInner::new(self, breakpoints, output, wakeup_state, blocking_waitpid)?;
        let result = tokio::runtime::Handle::current()
          .block_on(async move { inner.run(args, token.req_rx).await });
        if let Err(e) = &result {
          // The receiver may have been dropped while the tracer was shutting down.
          let _ = tx.send(TracerMessage::FatalError(e.to_string()));
        }
        result
      }
    });
    let tid = tid_rx.recv()?;
    // delay the creation of RunningTracer till we get tid
    let running_tracer = RunningTracer {
      tid,
      breakpoints: breakpoints_clone,
      req_tx,
      seccomp_bpf,
      blocking,
    };
    Ok((running_tracer, tracer_thread))
  }

  #[cfg(test)]
  fn attach_for_test(self, pid: Pid) -> tokio::task::JoinHandle<color_eyre::Result<()>> {
    let tx = self.msg_tx.clone();
    tokio::task::spawn_blocking(move || {
      let inner = TracerInner::new(
        self,
        Arc::new(RwLock::new(BTreeMap::new())),
        None,
        Arc::new(JobControlWakeupState::default()),
        Arc::new(AtomicBool::new(false)),
      )?;
      let result = inner.run_attached(pid);
      if let Err(e) = &result {
        let _ = tx.send(TracerMessage::FatalError(e.to_string()));
      }
      result
    })
  }
}

static BREAKPOINT_ID: AtomicU32 = AtomicU32::new(0);

#[doc(hidden)]
#[allow(unused)]
/// Only meant for tests
pub fn clear_breakpoint_id_counter() {
  BREAKPOINT_ID.store(0, Ordering::SeqCst);
}

/// Breakpoint management
impl RunningTracer {
  pub fn add_breakpoint(&self, breakpoint: BreakPoint) -> u32 {
    let id = BREAKPOINT_ID.fetch_add(1, Ordering::SeqCst);
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

  fn blocking_mode_notify_tracer(&self) -> Result<(), Errno> {
    notify_tracer_thread(self.tid)
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
    if self.blocking {
      self.blocking_mode_notify_tracer()?;
    }
    Ok(())
  }

  pub fn request_process_resume(&self, hit: BreakPointHit) -> color_eyre::Result<()> {
    self.req_tx.send(PendingRequest::ResumeProcess(hit))?;
    if self.blocking {
      self.blocking_mode_notify_tracer()?;
    }
    Ok(())
  }

  pub fn request_suspend_seccomp_bpf(&self, pid: Pid) -> color_eyre::Result<()> {
    trace!("received request to suspend {pid}'s seccomp-bpf filter");
    self.req_tx.send(PendingRequest::SuspendSeccompBpf(pid))?;
    if self.blocking {
      self.blocking_mode_notify_tracer()?;
    }
    Ok(())
  }

  pub fn request_termination(&self) -> color_eyre::Result<()> {
    self.req_tx.send(PendingRequest::TerminateTracer)?;
    if self.blocking {
      self.blocking_mode_notify_tracer()?;
    }
    Ok(())
  }

  pub fn seccomp_bpf(&self) -> bool {
    self.seccomp_bpf == SeccompBpf::On
  }

  /// Create a mock tracer for unit tests.
  #[doc(hidden)]
  pub fn mock() -> Self {
    let (req_tx, req_rx) = unbounded_channel();
    // Keep receiver alive so request_* calls don't fail in tests.
    std::mem::forget(req_rx);
    Self {
      tid: unsafe { pthread_self() },
      breakpoints: Arc::new(RwLock::new(BTreeMap::new())),
      req_tx,
      seccomp_bpf: SeccompBpf::Off,
      blocking: false,
    }
  }
}

impl Tracer {
  fn blocking(&self) -> bool {
    self.polling_interval.is_none()
  }
}
