//! Safe abstraction for PTRACE
//!
//! This is written mainly for solving https://github.com/kxxt/tracexec/issues/36
//!
//! `nix`'s ptrace have problem about RT signals: https://github.com/nix-rust/nix/issues/495

use std::{cell::Cell, marker::PhantomData, sync::MutexGuard};

use nix::{
  errno::Errno,
  sys::{
    ptrace::{self},
    wait::{waitpid, WaitPidFlag, WaitStatus},
  },
  unistd::Pid,
};
use tracing::trace;

mod guards;
mod syscall;
mod waitpid;

pub use guards::*;
pub use waitpid::*;

pub type PhantomUnsync = PhantomData<Cell<()>>;
pub type PhantomUnsend = PhantomData<MutexGuard<'static, ()>>;

#[derive(Debug)]
pub struct RecursivePtraceEngine {
  seccomp: bool,
  _unsync_marker: PhantomUnsync,
  _unsend_marker: PhantomUnsend,
  running: bool,
}

/// A recursive ptracer that works on a tracee and all of its children.
impl RecursivePtraceEngine {
  /// Create a new [`RecursivePtraceEngine`] for local thread.
  pub fn new(seccomp: bool) -> Self {
    Self {
      seccomp,
      _unsync_marker: PhantomData,
      _unsend_marker: PhantomData,
      running: false,
    }
  }

  pub fn seize_children_recursive(
    &mut self,
    tracee: Pid,
    mut options: nix::sys::ptrace::Options,
  ) -> Result<PtraceGroupStopGuard<'_>, Errno> {
    if self.running {
      return Err(Errno::EEXIST);
    } else {
      self.running = true;
    }
    // In this loop, the tracee is not traced yet.
    loop {
      let status = waitpid(tracee, Some(WaitPidFlag::WSTOPPED))?;
      match status {
        WaitStatus::Stopped(_, nix::sys::signal::SIGSTOP) => {
          break;
        }
        WaitStatus::Stopped(_, _) => {
          trace!("tracee stopped by other signal, continuing");
          continue;
        }
        _ => unreachable!(), // WSTOPPED wait for children that have been stopped by delivery of a signal.
      }
    }
    trace!("tracee stopped, setting options");
    use nix::sys::ptrace::Options;
    if self.seccomp {
      options |= Options::PTRACE_O_TRACESECCOMP;
    }
    ptrace::seize(
      tracee,
      options
        | Options::PTRACE_O_TRACEFORK
        | Options::PTRACE_O_TRACECLONE
        | Options::PTRACE_O_TRACEVFORK,
    )?;

    // Then we will observe a group stop resulting of the very SIGSTOP

    let status = waitpid::waitpid(self, Some(tracee), Some(WaitPidFlag::WSTOPPED))?;
    trace!("waitpid event: {status:?}");
    match status {
      PtraceWaitPidEvent::Signaled { .. } | PtraceWaitPidEvent::Exited { .. } => Err(Errno::ESRCH),
      PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Group(guard)) => Ok(guard),
      _ => unreachable!(),
    }
  }

  /// Following the convention on ptrace(2), this function expects a child that initiates a `PTRACE_TRACEME`
  /// request and raise a SIGSTOP signal.
  ///
  /// This function will wait until the child is in the signal delivery stop of SIGSTOP.
  /// If any other signal is raised for the tracee, this function
  #[allow(unused)]
  pub unsafe fn import_traceme_child(
    &mut self,
    tracee: Pid,
    mut options: nix::sys::ptrace::Options, // TODO: we shouldn't expose this.
  ) -> Result<PtraceSignalDeliveryStopGuard<'_>, Errno> {
    if self.running {
      return Err(Errno::EEXIST);
    } else {
      self.running = true;
    }
    loop {
      let status = waitpid(tracee, Some(WaitPidFlag::WSTOPPED))?;
      match status {
        WaitStatus::Stopped(_, nix::sys::signal::SIGSTOP) => {
          break;
        }
        WaitStatus::Stopped(_, signal) => {
          trace!("tracee stopped by other signal, delivering it...");
          ptrace::cont(tracee, signal)?;
        }
        _ => unreachable!(), // WSTOPPED wait for children that have been stopped by delivery of a signal.
      }
    }
    trace!("tracee stopped, setting options");
    use nix::sys::ptrace::Options;
    if self.seccomp {
      options |= Options::PTRACE_O_TRACESECCOMP;
    }
    ptrace::setoptions(
      tracee,
      options
        | Options::PTRACE_O_TRACEFORK
        | Options::PTRACE_O_TRACECLONE
        | Options::PTRACE_O_TRACEVFORK,
    )?;
    Ok(PtraceSignalDeliveryStopGuard {
      signal: nix::sys::signal::SIGSTOP.into(),
      guard: PtraceOpaqueStopGuard::new(self, tracee),
    })
  }

  pub fn next_event(
    &self,
    waitpid_flags: Option<WaitPidFlag>,
  ) -> Result<PtraceWaitPidEvent<'_>, Errno> {
    let event = waitpid::waitpid(self, None, waitpid_flags);
    if !matches!(event, Ok::<_, Errno>(PtraceWaitPidEvent::StillAlive)) {
      trace!("waitpid event: {:?}", event);
    }
    event
  }
}
