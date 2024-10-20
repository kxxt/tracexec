use nix::libc::user_regs_struct;

pub const NATIVE_AUDIT_ARCH: u32 = super::AUDIT_ARCH_X86_64;
pub const SYS_EXECVE_32: i32 = 11;
pub const SYS_EXECVEAT_32: i32 = 358;
pub const HAS_32BIT: bool = true;

// https://github.com/rust-lang/rfcs/blob/master/text/2195-really-tagged-unions.md
#[repr(C, u32)]
#[derive(Debug)]
#[expect(dead_code)] // Variants are constructed in unsafe code
pub enum Regs {
  X86(PtraceRegisters32),
  X64(PtraceRegisters64),
}

#[repr(u32)]
pub enum RegsTag {
  X86,
  X64,
}

#[repr(C)]
pub union RegsPayload {
  x86: PtraceRegisters32,
  x64: PtraceRegisters64,
}

#[repr(C)]
pub struct RegsRepr {
  pub tag: RegsTag,
  pub payload: RegsPayload,
}

pub type PtraceRegisters64 = user_regs_struct;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PtraceRegisters32 {
  ebx: u32,
  ecx: u32,
  edx: u32,
  esi: u32,
  edi: u32,
  ebp: u32,
  eax: u32,
  xds: u32,
  xes: u32,
  xfs: u32,
  xgs: u32,
  orig_eax: u32,
  eip: u32,
  xcs: u32,
  eflags: u32,
  esp: u32,
  xss: u32,
}

use super::RegsExt;

impl RegsExt for Regs {
  fn syscall_arg(&self, idx: usize, is_32bit: bool) -> usize {
    match self {
      Regs::X86(regs) => {
        debug_assert!(is_32bit);
        (match idx {
          0 => regs.ebx,
          1 => regs.ecx,
          2 => regs.edx,
          3 => regs.esi,
          4 => regs.edi,
          5 => unimplemented!(),
          _ => unreachable!(),
        } as usize)
      }
      Regs::X64(regs) => {
        if is_32bit {
          (match idx {
            0 => regs.rbx,
            1 => regs.rcx,
            2 => regs.rdx,
            3 => regs.rsi,
            4 => regs.rdi,
            5 => unimplemented!(),
            _ => unreachable!(),
          } as u32 as usize)
        } else {
          (match idx {
            0 => regs.rdi,
            1 => regs.rsi,
            2 => regs.rdx,
            3 => regs.r10,
            4 => regs.r8,
            5 => regs.r9,
            _ => unreachable!(),
          } as usize)
        }
      }
    }
  }

  fn syscall_ret(&self) -> isize {
    match self {
      Regs::X86(regs) => regs.eax as isize,
      Regs::X64(regs) => regs.rax as isize,
    }
  }
}
