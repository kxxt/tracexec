use cfg_if::cfg_if;
use nix::{errno::Errno, sys::ptrace, sys::signal::Signal, unistd::Pid};

use crate::arch::PtraceRegisters;

pub fn ptrace_syscall(pid: Pid, sig: Option<Signal>) -> Result<(), Errno> {
  match ptrace::syscall(pid, sig) {
    Err(Errno::ESRCH) => {
      log::info!("ptrace syscall failed: {pid}, ESRCH, child probably gone!");
      Ok(())
    }
    other => other,
  }
}

#[cfg(feature = "seccomp-bpf")]
pub fn ptrace_cont(pid: Pid, sig: Option<Signal>) -> Result<(), Errno> {
  match ptrace::cont(pid, sig) {
    Err(Errno::ESRCH) => {
      log::info!("ptrace cont failed: {pid}, ESRCH, child probably gone!");
      Ok(())
    }
    other => other,
  }
}

pub fn ptrace_getregs(pid: Pid) -> Result<PtraceRegisters, Errno> {
  // Don't use GETREGSET on x86_64.
  // In some cases(it usually happens several times at and after exec syscall exit),
  // we only got 68/216 bytes into `regs`, which seems unreasonable. Not sure why.
  cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
          ptrace::getregs(pid)
      } else {
          use nix::sys::ptrace::AddressType;

          let mut regs = std::mem::MaybeUninit::<PtraceRegisters>::uninit();
          let iovec = nix::libc::iovec {
              iov_base: regs.as_mut_ptr() as AddressType,
              iov_len: std::mem::size_of::<PtraceRegisters>(),
          };
          let ptrace_result = unsafe {
              nix::libc::ptrace(
                  nix::libc::PTRACE_GETREGSET,
                  pid.as_raw(),
                  nix::libc::NT_PRSTATUS,
                  &iovec as *const _ as *const nix::libc::c_void,
              )
          };
          let regs = if -1 == ptrace_result {
              let errno = nix::errno::Errno::last();
              return Err(errno);
          } else {
              assert_eq!(iovec.iov_len, std::mem::size_of::<PtraceRegisters>());
              unsafe { regs.assume_init() }
          };
          Ok(regs)
      }
  }
}
