// Adapted from nix::sys::wait, original copyright notice:

// The MIT License (MIT)
//
// Copyright (c) 2015 Carl Lerche + nix-rust Authors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.
#![allow(unused)]

use std::{fmt::Display, hint::black_box};

use nix::{
  errno::Errno,
  libc::{self, c_int, pid_t, SIGRTMIN, WSTOPSIG},
  sys::wait::WaitPidFlag,
  unistd::Pid,
};

use crate::ptrace::{
  guards::{
    PtraceCloneChildStopGuard, PtraceCloneParentStopGuard, PtraceExecStopGuard,
    PtraceExitStopGuard, PtraceGroupStopGuard, PtraceSignalDeliveryStopGuard,
  },
  PtraceSeccompStopGuard,
};

use super::{
  guards::{PtraceStopGuard, PtraceStopInnerGuard, PtraceSyscallStopGuard},
  RecursivePtraceEngine,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
  Standard(nix::sys::signal::Signal),
  Realtime(u8), // u8 is enough for Linux
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

impl Signal {
  pub(crate) fn from_raw(raw: c_int) -> Self {
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

#[derive(Debug)]
pub enum PtraceWaitPidEvent<'a> {
  Ptrace(PtraceStopGuard<'a>),
  Signaled { pid: Pid, signal: Signal },
  Exited { pid: Pid, code: i32 },
  Continued(#[allow(unused)] Pid),
  StillAlive,
}

impl<'a> PtraceWaitPidEvent<'a> {
  pub(crate) fn from_raw(
    engine: &'a RecursivePtraceEngine,
    pid: Pid,
    status: c_int,
  ) -> Result<Self, Errno> {
    Ok(if libc::WIFEXITED(status) {
      PtraceWaitPidEvent::Exited {
        pid,
        code: libc::WEXITSTATUS(status),
      }
    } else if libc::WIFSIGNALED(status) {
      PtraceWaitPidEvent::Signaled {
        pid,
        signal: Signal::from_raw(libc::WTERMSIG(status)),
      }
    } else if libc::WIFSTOPPED(status) {
      // PTRACE_O_TRACESYSGOOD
      let stopsig = libc::WSTOPSIG(status);
      if stopsig == libc::SIGTRAP | 0x80 {
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Syscall(PtraceSyscallStopGuard {
          guard: PtraceStopInnerGuard::new(engine, pid),
        }))
      } else {
        let additional = status >> 16;
        if additional == 0 {
          // Not a special ptrace stop event.

          // Use PTRACE_GETSIGINFO to solve ambiguity
          // Signal Delivery Stop
          let signal = Signal::from_raw(stopsig);
          match signal {
            // Only these four signals can be group-stop
            Signal::Standard(nix::sys::signal::SIGSTOP)
            | Signal::Standard(nix::sys::signal::SIGTSTP)
            | Signal::Standard(nix::sys::signal::SIGTTIN)
            | Signal::Standard(nix::sys::signal::SIGTTOU) => {
              // Ambiguity
              let siginfo = nix::sys::ptrace::getsiginfo(pid);
              match siginfo {
                // First, we check special SIGSTOP
                Ok(siginfo)
                  if signal == Signal::Standard(nix::sys::signal::SIGSTOP)
                    && unsafe { siginfo.si_pid() == 0 } =>
                {
                  // This is a PTRACE event disguised under SIGSTOP
                  // e.g. PTRACE_O_TRACECLONE generates this event for newly cloned process
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneChild(
                    PtraceCloneChildStopGuard {
                      guard: PtraceStopInnerGuard::new(engine, pid),
                    },
                  ))
                }
                // Then, if we successfully get siginfo, this is a normal signal
                Ok(_) => {
                  // This signal is sent by kill/sigqueue
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(
                    PtraceSignalDeliveryStopGuard {
                      signal,
                      guard: PtraceStopInnerGuard::new(engine, pid),
                    },
                  ))
                }
                // Otherwise, if we see EINVAL, this is a group-stop
                Err(Errno::EINVAL) => {
                  // group-stop
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Group(PtraceGroupStopGuard {
                    guard: PtraceStopInnerGuard::new(engine, pid),
                  }))
                }
                // The child is killed before we get to run getsiginfo (very little chance)
                // In such case we just report a signal delivery stop
                Err(Errno::ESRCH) => PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(
                  PtraceSignalDeliveryStopGuard {
                    signal,
                    guard: PtraceStopInnerGuard::new(engine, pid),
                  },
                )),
                // Could this ever happen?
                Err(other) => return Err(other),
              }
            }
            _ => PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(
              PtraceSignalDeliveryStopGuard {
                signal,
                guard: PtraceStopInnerGuard::new(engine, pid),
              },
            )),
          }
        } else {
          // A special ptrace stop
          debug_assert_eq!(WSTOPSIG(status), libc::SIGTRAP);
          match additional {
            libc::PTRACE_EVENT_SECCOMP => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Seccomp(PtraceSeccompStopGuard {
                guard: PtraceStopInnerGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_EXEC => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Exec(PtraceExecStopGuard {
                former_tid: nix::sys::ptrace::getevent(pid).map(|x| Pid::from_raw(x as pid_t)),
                guard: PtraceStopInnerGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_EXIT => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Exit(PtraceExitStopGuard {
                status: nix::sys::ptrace::getevent(pid).map(|x| x as c_int),
                guard: PtraceStopInnerGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_CLONE | libc::PTRACE_EVENT_FORK | libc::PTRACE_EVENT_VFORK => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneParent(PtraceCloneParentStopGuard {
                child: nix::sys::ptrace::getevent(pid).map(|x| Pid::from_raw(x as pid_t)),
                guard: PtraceStopInnerGuard::new(engine, pid),
              }))
            }
            _ => unimplemented!(),
          }
        }
      }
    } else {
      assert!(libc::WIFCONTINUED(status));
      Self::Continued(pid)
    })
  }
}

/// Wait for a process to change status
///
/// See also [waitpid(2)](https://pubs.opengroup.org/onlinepubs/9699919799/functions/waitpid.html)
pub(super) fn waitpid<P: Into<Option<Pid>>>(
  engine: &RecursivePtraceEngine,
  pid: P,
  options: Option<WaitPidFlag>,
) -> Result<PtraceWaitPidEvent<'_>, Errno> {
  let mut status: i32 = black_box(0);

  let option_bits = match options {
    Some(bits) => bits.bits(),
    None => 0,
  };

  let res = unsafe {
    nix::libc::waitpid(
      pid.into().unwrap_or_else(|| Pid::from_raw(-1)).into(),
      &mut status as *mut c_int,
      option_bits,
    )
  };

  match Errno::result(res)? {
    0 => Ok(PtraceWaitPidEvent::StillAlive),
    res => PtraceWaitPidEvent::from_raw(engine, Pid::from_raw(res), status),
  }
}
