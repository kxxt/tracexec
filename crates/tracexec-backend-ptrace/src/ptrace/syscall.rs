#![allow(unused)]

use cfg_if::cfg_if;
use nix::libc::{
  SYS_execve,
  SYS_execveat,
};

use crate::arch::{
  HAS_32BIT,
  NATIVE_AUDIT_ARCH,
};

#[allow(unused)]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditArch(u32);

impl AuditArch {
  // FIXME: validate it
  pub fn from_raw(value: u32) -> Self {
    Self(value)
  }

  pub fn is_32bit(&self) -> bool {
    if HAS_32BIT {
      // x86_64
      // FIXME: x32 ABI
      NATIVE_AUDIT_ARCH != self.0
    } else {
      // aarch64, riscv64
      false
    }
  }
}

#[allow(unused)]
pub struct SyscallInfo {
  pub(super) arch: AuditArch,
  // ip: c_long,
  // sp: c_long,
  pub(super) data: SyscallInfoData,
}

impl SyscallInfo {
  pub fn arch(&self) -> AuditArch {
    self.arch
  }

  pub fn syscall_result(&self) -> Option<i64> {
    if let SyscallInfoData::Exit { retval, .. } = &self.data {
      Some(*retval)
    } else {
      None
    }
  }

  pub fn syscall_number(&self) -> Option<u64> {
    if let SyscallInfoData::Entry { syscall_nr, .. } | SyscallInfoData::Seccomp { syscall_nr, .. } =
      &self.data
    {
      Some(*syscall_nr)
    } else {
      None
    }
  }

  pub fn is_execve(&self) -> Option<bool> {
    if let SyscallInfoData::Entry { syscall_nr, .. } | SyscallInfoData::Seccomp { syscall_nr, .. } =
      &self.data
    {
      cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
          Some((self.arch == AuditArch::from_raw(crate::arch::AUDIT_ARCH_X86_64) && *syscall_nr == SYS_execve as u64) ||
          (self.arch == AuditArch::from_raw(crate::arch::AUDIT_ARCH_I386) && *syscall_nr == crate::arch::SYS_EXECVE_32 as u64))
        } else {
          Some(self.arch == AuditArch::from_raw(NATIVE_AUDIT_ARCH) && *syscall_nr == SYS_execve as u64)
        }
      }
    } else {
      None
    }
  }

  pub fn is_execveat(&self) -> Option<bool> {
    if let SyscallInfoData::Entry { syscall_nr, .. } | SyscallInfoData::Seccomp { syscall_nr, .. } =
      &self.data
    {
      cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
          Some((self.arch == AuditArch::from_raw(crate::arch::AUDIT_ARCH_X86_64) && *syscall_nr == SYS_execveat as u64) ||
          (self.arch == AuditArch::from_raw(crate::arch::AUDIT_ARCH_I386) && *syscall_nr == crate::arch::SYS_EXECVEAT_32 as u64))
        } else {
          Some(self.arch == AuditArch::from_raw(NATIVE_AUDIT_ARCH) && *syscall_nr == SYS_execveat as u64)
        }
      }
    } else {
      None
    }
  }
}

#[allow(unused)]
pub enum SyscallInfoData {
  Entry {
    syscall_nr: u64,
    args: [u64; 6],
  },
  Exit {
    retval: i64,
    is_error: bool,
  },
  Seccomp {
    syscall_nr: u64,
    args: [u64; 6],
    ret_data: u32,
  },
}
