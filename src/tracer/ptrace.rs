#![allow(unused)]

use std::{mem::MaybeUninit, ptr::addr_of_mut};

use cfg_if::cfg_if;
use nix::{
  errno::Errno,
  libc::{
    ptrace_syscall_info, SYS_execve, SYS_execveat, PTRACE_GET_SYSCALL_INFO,
    PTRACE_SYSCALL_INFO_ENTRY, PTRACE_SYSCALL_INFO_EXIT, PTRACE_SYSCALL_INFO_SECCOMP,
  },
  unistd::Pid,
};

use crate::arch::{Regs, RegsPayload, RegsRepr, HAS_32BIT, NATIVE_AUDIT_ARCH};

pub use nix::sys::ptrace::*;

pub struct SyscallInfo {
  pub arch: u32,
  pub number: i64,
}

impl SyscallInfo {
  /// Returns true if this syscall is 32bit.
  ///
  /// It is possible for a 64bit process to make a 32bit syscall,
  /// resulting in X64 ptregs but with 32bit semantics
  pub fn is_32bit(&self) -> bool {
    if HAS_32BIT {
      // FIXME: x32 ABI
      NATIVE_AUDIT_ARCH != self.arch
    } else {
      false
    }
  }

  pub fn is_execve(&self) -> bool {
    cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
        use crate::arch;
        (self.arch == arch::AUDIT_ARCH_X86_64 && self.number == SYS_execve) ||
        (self.arch == arch::AUDIT_ARCH_I386 && self.number == arch::SYS_EXECVE_32 as i64)
      } else {
        self.arch == NATIVE_AUDIT_ARCH && self.number == SYS_execve
      }
    }
  }

  pub fn is_execveat(&self) -> bool {
    cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
        use crate::arch;
        (self.arch == arch::AUDIT_ARCH_X86_64 && self.number == SYS_execveat) ||
        (self.arch == arch::AUDIT_ARCH_I386 && self.number == arch::SYS_EXECVEAT_32 as i64)
      } else {
        self.arch == NATIVE_AUDIT_ARCH && self.number == SYS_execveat
      }
    }
  }
}

/// Get [`SyscallInfo`] on ptrace syscall entry/seccomp stop
///
/// # Precondition
///
/// The caller is the tracer thread and at the syscall entry/seccomp stop.
pub fn syscall_entry_info(pid: Pid) -> Result<SyscallInfo, Errno> {
  let mut info = MaybeUninit::<ptrace_syscall_info>::uninit();
  let info = unsafe {
    let ret = nix::libc::ptrace(
      PTRACE_GET_SYSCALL_INFO,
      pid.as_raw(),
      size_of::<ptrace_syscall_info>(),
      info.as_mut_ptr(),
    );
    if ret < 0 {
      return Err(Errno::last());
    } else {
      info.assume_init()
    }
  };
  let arch = info.arch;
  let number = if info.op == PTRACE_SYSCALL_INFO_ENTRY {
    unsafe { info.u.entry.nr }
  } else if info.op == PTRACE_SYSCALL_INFO_SECCOMP {
    unsafe { info.u.seccomp.nr }
  } else {
    // Not syscall entry/seccomp stop
    return Err(Errno::EINVAL);
  } as i64;
  Ok(SyscallInfo { arch, number })
}

/// Get syscall result on ptrace syscall exit stop
///
/// # Precondition
///
/// The caller is the tracer thread and at the syscall exit stop.
pub fn syscall_exit_result(pid: Pid) -> Result<isize, Errno> {
  let mut info = MaybeUninit::<ptrace_syscall_info>::uninit();
  let info = unsafe {
    let ret = nix::libc::ptrace(
      PTRACE_GET_SYSCALL_INFO,
      pid.as_raw(),
      size_of::<ptrace_syscall_info>(),
      info.as_mut_ptr(),
    );
    if ret < 0 {
      return Err(Errno::last());
    } else {
      info.assume_init()
    }
  };
  if info.op == PTRACE_SYSCALL_INFO_EXIT {
    Ok(unsafe { info.u.exit.sval } as isize)
  } else {
    Err(Errno::EINVAL)
  }
}

pub fn ptrace_getregs(pid: Pid) -> Result<Regs, Errno> {
  // https://github.com/torvalds/linux/blob/v6.9/include/uapi/linux/elf.h#L378
  // libc crate doesn't provide this constant when using musl libc.
  const NT_PRSTATUS: std::ffi::c_int = 1;

  use nix::sys::ptrace::AddressType;

  let mut regs = std::mem::MaybeUninit::<Regs>::uninit();
  let dest: *mut RegsRepr = unsafe { std::mem::transmute(regs.as_mut_ptr()) };
  let mut iovec = nix::libc::iovec {
    iov_base: unsafe { addr_of_mut!((*dest).payload) } as AddressType,
    iov_len: std::mem::size_of::<RegsPayload>(),
  };
  let ptrace_result = unsafe {
    nix::libc::ptrace(
      nix::libc::PTRACE_GETREGSET,
      pid.as_raw(),
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
          SIZE_OF_REGS32 => unsafe { addr_of_mut!((*dest).tag).write(crate::arch::RegsTag::X86); }
          SIZE_OF_REGS64 => unsafe { addr_of_mut!((*dest).tag).write(crate::arch::RegsTag::X64); }
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
