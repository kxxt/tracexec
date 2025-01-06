//! Safe abstraction for PTRACE
//!
//! This is written mainly for solving https://github.com/kxxt/tracexec/issues/36
//!
//! `nix`'s ptrace have problem about RT signals: https://github.com/nix-rust/nix/issues/495
#![allow(unused)]

use std::{ffi::c_int, mem::MaybeUninit};

use cfg_if::cfg_if;
use either::Either;
use nix::{
  errno::Errno,
  libc::{
    ptrace_syscall_info, PTRACE_GET_SYSCALL_INFO, PTRACE_SYSCALL_INFO_ENTRY,
    PTRACE_SYSCALL_INFO_EXIT, PTRACE_SYSCALL_INFO_SECCOMP,
  },
  sys::ptrace::AddressType,
  unistd::Pid,
};
use tracing::{info, trace};

use crate::arch::{Regs, RegsPayload, RegsRepr};

use super::{
  syscall::{AuditArch, SyscallInfo, SyscallInfoData},
  waitpid::Signal,
  RecursivePtraceEngine,
};
mod private {
  pub trait Sealed {}
}

/// Quoting the man page:
///
/// > For operations other than PTRACE_ATTACH, PTRACE_SEIZE, PTRACE_INTERRUPT, and
/// > PTRACE_KILL, the tracee must be stopped.
///
/// This trait provides methods that requires a stopped tracee
pub trait PtraceStop: private::Sealed + Sized {
  /// Inspect tracee's memory at `addr` as type `T`.
  #[allow(unused)]
  unsafe fn inspect<T>(&self, addr: AddressType) -> T {
    todo!()
  }

  fn get_general_registers(&self) -> Result<Regs, Errno> {
    // https://github.com/torvalds/linux/blob/v6.9/include/uapi/linux/elf.h#L378
    // libc crate doesn't provide this constant when using musl libc.
    const NT_PRSTATUS: std::ffi::c_int = 1;

    use nix::sys::ptrace::AddressType;

    let mut regs = std::mem::MaybeUninit::<Regs>::uninit();
    let dest: *mut RegsRepr = unsafe { std::mem::transmute(regs.as_mut_ptr()) };
    let mut iovec = nix::libc::iovec {
      iov_base: unsafe { &raw mut (*dest).payload } as AddressType,
      iov_len: std::mem::size_of::<RegsPayload>(),
    };
    let ptrace_result = unsafe {
      nix::libc::ptrace(
        nix::libc::PTRACE_GETREGSET,
        self.pid().as_raw(),
        NT_PRSTATUS,
        &mut iovec,
      )
    };
    let regs = if ptrace_result < 0 {
      let errno = nix::errno::Errno::last();
      return Err(errno);
    } else {
      cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
          const SIZE_OF_REGS64: usize = size_of::<crate::arch::PtraceRegisters64>();
          const SIZE_OF_REGS32: usize = size_of::<crate::arch::PtraceRegisters32>();
          match iovec.iov_len {
            SIZE_OF_REGS32 => unsafe { (&raw mut (*dest).tag).write(crate::arch::RegsTag::X86); }
            SIZE_OF_REGS64 => unsafe { (&raw mut (*dest).tag).write(crate::arch::RegsTag::X64); }
            size => panic!("Invalid length {size} of user_regs_struct!")
          }
        } else if #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))] {
          assert_eq!(iovec.iov_len, std::mem::size_of::<RegsPayload>());
        } else {
          compile_error!("Please update the code for your architecture!");
        }
      }
      unsafe { regs.assume_init() }
    };
    Ok(regs)
  }

  fn seccomp_aware_cont_syscall(self, ignore_esrch: bool) -> Result<(), Errno> {
    if self.seccomp() {
      self.cont(ignore_esrch)
    } else {
      self.cont_syscall(ignore_esrch)
    }
  }

  #[inline(always)]
  fn cont(self, ignore_esrch: bool) -> Result<(), Errno> {
    let pid = self.pid();
    let result = nix::sys::ptrace::cont(self.pid(), None);
    if ignore_esrch {
      match result {
        Err(Errno::ESRCH) => {
          info!("seccomp_aware_cont_syscall failed: {pid}, ESRCH, child gone!");
          Ok(())
        }
        other => other,
      }
    } else {
      result
    }
  }

  #[inline(always)]
  fn cont_syscall(self, ignore_esrch: bool) -> Result<(), Errno> {
    let pid = self.pid();
    let result = nix::sys::ptrace::syscall(self.pid(), None);
    if ignore_esrch {
      match result {
        Err(Errno::ESRCH) => {
          info!("seccomp_aware_cont_syscall failed: {pid}, ESRCH, child gone!");
          Ok(())
        }
        other => other,
      }
    } else {
      result
    }
  }

  fn detach(self) -> Result<(), Errno> {
    nix::sys::ptrace::detach(self.pid(), None)
  }

  /// TODO: only allow this for PTRACE_SEIZE
  fn listen(&self, ignore_esrch: bool) -> Result<(), Errno> {
    let pid = self.pid();
    trace!("put {pid} into listen state");
    let result = unsafe {
      Errno::result(nix::libc::ptrace(
        nix::sys::ptrace::Request::PTRACE_LISTEN as nix::sys::ptrace::RequestType,
        nix::libc::pid_t::from(pid.as_raw()),
        std::ptr::null_mut::<AddressType>(),
        0,
      ))
      .map(|_| ())
    };
    if ignore_esrch {
      match result {
        Err(Errno::ESRCH) => {
          info!("seccomp_aware_cont_syscall failed: {pid}, ESRCH, child gone!");
          Ok(())
        }
        other => other,
      }
    } else {
      result
    }
  }

  fn pid(&self) -> Pid;

  fn seccomp(&self) -> bool;
}

#[derive(Debug)]
pub struct PtraceOpaqueStopGuard<'a> {
  pub(super) pid: Pid,
  pub(super) engine: &'a RecursivePtraceEngine,
}

impl private::Sealed for PtraceOpaqueStopGuard<'_> {}
impl PtraceStop for PtraceOpaqueStopGuard<'_> {
  fn pid(&self) -> Pid {
    self.pid
  }

  fn seccomp(&self) -> bool {
    self.engine.seccomp
  }
}

impl PartialEq for PtraceOpaqueStopGuard<'_> {
  fn eq(&self, other: &Self) -> bool {
    self.pid == other.pid && std::ptr::eq(self.engine, other.engine)
  }
}

impl<'a> PtraceOpaqueStopGuard<'a> {
  pub(super) fn new(engine: &'a RecursivePtraceEngine, pid: Pid) -> Self {
    Self { pid, engine }
  }

  pub fn seccomp(&self) -> bool {
    self.engine.seccomp
  }
}

#[derive(Debug)]
pub struct PtraceSyscallStopGuard<'a> {
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

#[derive(Debug)]
pub struct PtraceSignalDeliveryStopGuard<'a> {
  pub(super) signal: Signal,
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

impl PtraceSignalDeliveryStopGuard<'_> {
  pub fn signal(&self) -> Signal {
    self.signal
  }

  #[allow(unused)]
  pub fn deliver_cont(self) -> Result<(), Errno> {
    let sig = self.signal;
    self.injected_cont(sig)
  }

  #[allow(unused)]
  pub fn deliver_cont_syscall(self) -> Result<(), Errno> {
    let sig = self.signal;
    self.injected_cont_syscall(sig)
  }

  pub fn seccomp_aware_deliver_cont_syscall(self, ignore_esrch: bool) -> Result<(), Errno> {
    let sig = self.signal;
    let result = self.injected_cont_syscall(sig);
    if ignore_esrch {
      match result {
        Err(Errno::ESRCH) => {
          // info!("seccomp_aware_deliver_cont_syscall failed: ESRCH, child gone!");
          Ok(())
        }
        other => other,
      }
    } else {
      result
    }
  }

  #[allow(unused)]
  pub fn deliver_detach(self) -> Result<(), Errno> {
    let sig = self.signal;
    self.injected_detach(sig)
  }

  pub fn injected_cont(self, sig: Signal) -> Result<(), Errno> {
    unsafe {
      Errno::result(nix::libc::ptrace(
        nix::sys::ptrace::Request::PTRACE_CONT as nix::sys::ptrace::RequestType,
        nix::libc::pid_t::from(self.pid().as_raw()),
        std::ptr::null_mut::<AddressType>(),
        sig.as_raw(),
      ))
      .map(|_| ())
    }
  }

  pub fn injected_cont_syscall(self, sig: Signal) -> Result<(), Errno> {
    unsafe {
      Errno::result(nix::libc::ptrace(
        nix::sys::ptrace::Request::PTRACE_SYSCALL as nix::sys::ptrace::RequestType,
        nix::libc::pid_t::from(self.pid().as_raw()),
        std::ptr::null_mut::<AddressType>(),
        sig.as_raw(),
      ))
      .map(|_| ())
    }
  }

  pub fn injected_detach(self, sig: Signal) -> Result<(), Errno> {
    unsafe {
      Errno::result(nix::libc::ptrace(
        nix::sys::ptrace::Request::PTRACE_DETACH as nix::sys::ptrace::RequestType,
        nix::libc::pid_t::from(self.pid().as_raw()),
        std::ptr::null_mut::<AddressType>(),
        sig.as_raw(),
      ))
      .map(|_| ())
    }
  }
}

#[derive(Debug)]
pub struct PtraceCloneParentStopGuard<'a> {
  pub(super) child: Result<Pid, Errno>,
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

impl PtraceCloneParentStopGuard<'_> {
  pub fn child(&self) -> Result<Pid, Errno> {
    self.child
  }
}

#[derive(Debug)]
pub struct PtraceCloneChildStopGuard<'a> {
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

#[derive(Debug)]
pub struct PtraceExitStopGuard<'a> {
  #[allow(unused)]
  pub(super) status: Result<c_int, Errno>,
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

#[derive(Debug)]
pub struct PtraceExecStopGuard<'a> {
  #[allow(unused)]
  pub(super) former_tid: Result<Pid, Errno>,
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

#[derive(Debug)]
pub struct PtraceSeccompStopGuard<'a> {
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

#[derive(Debug)]
pub struct PtraceGroupStopGuard<'a> {
  pub(super) signal: Signal,
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

#[derive(Debug)]
pub struct PtraceInterruptStopGuard<'a> {
  pub(super) guard: PtraceOpaqueStopGuard<'a>,
}

macro_rules! impl_ptrace_stop {
    ($($kind:ident($t:ident<$life:lifetime>)),*) => {
      $(
        impl<$life> private::Sealed for $t<$life> {}
        impl<$life> PtraceStop for $t<$life> {
          fn pid(&self) -> Pid {
            self.guard.pid
          }

          fn seccomp(&self) -> bool {
            self.guard.seccomp()
          }
        }
        impl<$life> From<$t<$life>> for PtraceStopGuard<$life> {
          fn from(value: $t<$life>) -> Self {
            Self::$kind(value)
          }
        }
      )*
    };
}

impl_ptrace_stop!(
  // syscall-stop
  Syscall(PtraceSyscallStopGuard<'a>),
  // signal-delivery-stop
  SignalDelivery(PtraceSignalDeliveryStopGuard<'a>),
  CloneChild(PtraceCloneChildStopGuard<'a>),
  // group-stop
  Group(PtraceGroupStopGuard<'a>),
  // ptrace event stops
  CloneParent(PtraceCloneParentStopGuard<'a>),
  Exec(PtraceExecStopGuard<'a>),
  Exit(PtraceExitStopGuard<'a>),
  Seccomp(PtraceSeccompStopGuard<'a>),
  Interrupt(PtraceInterruptStopGuard<'a>)
);

#[derive(Debug)]
pub enum PtraceStopGuard<'a> {
  Syscall(PtraceSyscallStopGuard<'a>),
  /// signal delivery stop
  ///
  /// Note that in rare cases the child dies before we could
  /// classify this signal delivery stop into a group stop
  /// or the child part of a clone stop.
  SignalDelivery(PtraceSignalDeliveryStopGuard<'a>),
  /// The stop that happens when a newly attached child
  /// gets stopped by SIGSTOP (traceme) or by PTRACE_EVENT_STOP
  /// with SIGTRAP (seize).
  ///
  /// Note that in the latter case, false positive might be reported.
  /// It is not sure whether this is a kernel bug
  /// or some undocumented cases for PTRACE_EVENT_STOP.
  CloneChild(PtraceCloneChildStopGuard<'a>),
  Group(PtraceGroupStopGuard<'a>),
  CloneParent(PtraceCloneParentStopGuard<'a>),
  Exec(PtraceExecStopGuard<'a>),
  Exit(PtraceExitStopGuard<'a>),
  Seccomp(PtraceSeccompStopGuard<'a>),
  Interrupt(PtraceInterruptStopGuard<'a>),
}

impl private::Sealed for PtraceStopGuard<'_> {}

impl PtraceStop for PtraceStopGuard<'_> {
  #[inline(always)]
  fn pid(&self) -> Pid {
    match self {
      PtraceStopGuard::Syscall(guard) => guard.pid(),
      PtraceStopGuard::SignalDelivery(guard) => guard.pid(),
      PtraceStopGuard::CloneChild(guard) => guard.pid(),
      PtraceStopGuard::Group(guard) => guard.pid(),
      PtraceStopGuard::CloneParent(guard) => guard.pid(),
      PtraceStopGuard::Exec(guard) => guard.pid(),
      PtraceStopGuard::Exit(guard) => guard.pid(),
      PtraceStopGuard::Seccomp(guard) => guard.pid(),
      PtraceStopGuard::Interrupt(guard) => guard.pid(),
    }
  }

  #[inline(always)]
  fn seccomp(&self) -> bool {
    match self {
      PtraceStopGuard::Syscall(guard) => guard.seccomp(),
      PtraceStopGuard::SignalDelivery(guard) => guard.seccomp(),
      PtraceStopGuard::CloneChild(guard) => guard.seccomp(),
      PtraceStopGuard::Group(guard) => guard.seccomp(),
      PtraceStopGuard::CloneParent(guard) => guard.seccomp(),
      PtraceStopGuard::Exec(guard) => guard.seccomp(),
      PtraceStopGuard::Exit(guard) => guard.seccomp(),
      PtraceStopGuard::Seccomp(guard) => guard.seccomp(),
      PtraceStopGuard::Interrupt(guard) => guard.seccomp(),
    }
  }
}

impl<L, R> private::Sealed for Either<L, R>
where
  L: private::Sealed,
  R: private::Sealed,
{
}

impl<L, R> PtraceStop for Either<L, R>
where
  L: PtraceStop,
  R: PtraceStop,
{
  #[inline(always)]
  fn pid(&self) -> Pid {
    match self {
      Self::Left(l) => l.pid(),
      Self::Right(r) => r.pid(),
    }
  }

  #[inline(always)]
  fn seccomp(&self) -> bool {
    match self {
      Self::Left(l) => l.seccomp(),
      Self::Right(r) => r.seccomp(),
    }
  }
}

pub trait PtraceSyscallLikeStop: PtraceStop {
  fn raw_syscall_info(&self) -> Result<ptrace_syscall_info, Errno> {
    let mut info = MaybeUninit::<ptrace_syscall_info>::uninit();
    let info = unsafe {
      let ret = nix::libc::ptrace(
        PTRACE_GET_SYSCALL_INFO,
        self.pid().as_raw(),
        size_of::<ptrace_syscall_info>(),
        info.as_mut_ptr(),
      );
      if ret < 0 {
        return Err(Errno::last());
      } else {
        info.assume_init()
      }
    };
    Ok(info)
  }

  fn syscall_info(&self) -> Result<SyscallInfo, Errno> {
    let raw = self.raw_syscall_info()?;
    Ok(SyscallInfo {
      arch: AuditArch::from_raw(raw.arch),
      data: unsafe {
        match raw.op {
          PTRACE_SYSCALL_INFO_ENTRY => SyscallInfoData::Entry {
            syscall_nr: raw.u.entry.nr,
            args: raw.u.entry.args,
          },
          PTRACE_SYSCALL_INFO_SECCOMP => SyscallInfoData::Seccomp {
            syscall_nr: raw.u.seccomp.nr,
            args: raw.u.seccomp.args,
            ret_data: raw.u.seccomp.ret_data,
          },
          PTRACE_SYSCALL_INFO_EXIT => SyscallInfoData::Exit {
            retval: raw.u.exit.sval,
            is_error: raw.u.exit.is_error != 0,
          },
          _ => unreachable!(),
        }
      },
    })
  }
}

impl PtraceSyscallLikeStop for PtraceSyscallStopGuard<'_> {}
impl PtraceSyscallLikeStop for PtraceSeccompStopGuard<'_> {}

impl<L, R> PtraceSyscallLikeStop for Either<L, R>
where
  L: PtraceSyscallLikeStop,
  R: PtraceSyscallLikeStop,
{
}
