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
