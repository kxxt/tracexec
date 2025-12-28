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
  libc::{self, SIGRTMIN, WSTOPSIG, c_int, pid_t},
  sys::{signal, wait::WaitPidFlag},
  unistd::Pid,
};
use tracing::trace;

use tracexec_core::tracer::Signal;

use crate::ptrace::{
  PtraceSeccompStopGuard,
  guards::{
    PtraceCloneChildStopGuard, PtraceCloneParentStopGuard, PtraceExecStopGuard,
    PtraceExitStopGuard, PtraceGroupStopGuard, PtraceSignalDeliveryStopGuard,
  },
};

use super::{
  PtraceInterruptStopGuard, RecursivePtraceEngine,
  guards::{PtraceOpaqueStopGuard, PtraceStopGuard, PtraceSyscallStopGuard},
};

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
          guard: PtraceOpaqueStopGuard::new(engine, pid),
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
            Signal::Standard(signal::SIGSTOP)
            | Signal::Standard(signal::SIGTSTP)
            | Signal::Standard(signal::SIGTTIN)
            | Signal::Standard(signal::SIGTTOU) => {
              // Ambiguity
              let siginfo = nix::sys::ptrace::getsiginfo(pid);
              match siginfo {
                // First, we check special SIGSTOP
                Ok(siginfo)
                  if signal == Signal::Standard(signal::SIGSTOP)
                    && unsafe { siginfo.si_pid() == 0 } =>
                {
                  // This is a PTRACE event disguised under SIGSTOP
                  // e.g. PTRACE_O_TRACECLONE generates this event for newly cloned process
                  trace!("clone child event as sigstop");
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneChild(
                    PtraceCloneChildStopGuard {
                      guard: PtraceOpaqueStopGuard::new(engine, pid),
                    },
                  ))
                }
                // Then, if we successfully get siginfo, this is a normal signal
                Ok(_) => {
                  // This signal is sent by kill/sigqueue
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(
                    PtraceSignalDeliveryStopGuard {
                      signal,
                      guard: PtraceOpaqueStopGuard::new(engine, pid),
                    },
                  ))
                }
                // Otherwise, if we see EINVAL, this is a group-stop
                Err(Errno::EINVAL) => {
                  // group-stop
                  trace!("group stop, unseized");
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Group(PtraceGroupStopGuard {
                    signal,
                    guard: PtraceOpaqueStopGuard::new(engine, pid),
                  }))
                }
                // The child is killed before we get to run getsiginfo (very little chance)
                // In such case we just report a signal delivery stop
                Err(Errno::ESRCH) => PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(
                  PtraceSignalDeliveryStopGuard {
                    signal,
                    guard: PtraceOpaqueStopGuard::new(engine, pid),
                  },
                )),
                // Could this ever happen?
                Err(other) => return Err(other),
              }
            }
            _ => PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(
              PtraceSignalDeliveryStopGuard {
                signal,
                guard: PtraceOpaqueStopGuard::new(engine, pid),
              },
            )),
          }
        } else {
          // A special ptrace stop
          match additional {
            libc::PTRACE_EVENT_SECCOMP => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Seccomp(PtraceSeccompStopGuard {
                guard: PtraceOpaqueStopGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_EXEC => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Exec(PtraceExecStopGuard {
                former_tid: nix::sys::ptrace::getevent(pid).map(|x| Pid::from_raw(x as pid_t)),
                guard: PtraceOpaqueStopGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_EXIT => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Exit(PtraceExitStopGuard {
                status: nix::sys::ptrace::getevent(pid).map(|x| x as c_int),
                guard: PtraceOpaqueStopGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_CLONE | libc::PTRACE_EVENT_FORK | libc::PTRACE_EVENT_VFORK => {
              PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneParent(PtraceCloneParentStopGuard {
                child: nix::sys::ptrace::getevent(pid).map(|x| Pid::from_raw(x as pid_t)),
                guard: PtraceOpaqueStopGuard::new(engine, pid),
              }))
            }
            libc::PTRACE_EVENT_STOP => {
              let sig = Signal::from_raw(WSTOPSIG(status));
              match sig {
                Signal::Standard(signal::SIGTRAP) => {
                  if nix::sys::ptrace::getsiginfo(pid) == Err(Errno::EINVAL) {
                    //  PTRACE_INTERRUPT
                    PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Interrupt(
                      PtraceInterruptStopGuard {
                        guard: PtraceOpaqueStopGuard::new(engine, pid),
                      },
                    ))
                  } else {
                    // Newly cloned child
                    trace!(
                      "unsure child {pid}, eventmsg: {:?}, siginfo: {:?}",
                      nix::sys::ptrace::getevent(pid),
                      nix::sys::ptrace::getsiginfo(pid)
                    );
                    PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneChild(
                      PtraceCloneChildStopGuard {
                        guard: PtraceOpaqueStopGuard::new(engine, pid),
                      },
                    ))
                  }
                }
                // Only these four signals can be group-stop
                Signal::Standard(signal::SIGSTOP)
                | Signal::Standard(signal::SIGTSTP)
                | Signal::Standard(signal::SIGTTIN)
                | Signal::Standard(signal::SIGTTOU) => {
                  PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Group(PtraceGroupStopGuard {
                    signal: sig,
                    guard: PtraceOpaqueStopGuard::new(engine, pid),
                  }))
                }
                _ => unimplemented!("ptrace_interrupt"),
              }
            }
            _ => unreachable!(),
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
