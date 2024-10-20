use super::RegsExt;
use nix::libc::user_regs_struct;

pub const NATIVE_AUDIT_ARCH: u32 = super::AUDIT_ARCH_RISCV64;
pub const HAS_32BIT: bool = false;

pub type Regs = user_regs_struct;
pub type RegsPayload = Regs;
#[repr(transparent)]
pub struct RegsRepr {
  pub payload: RegsPayload,
}

impl RegsExt for Regs {
  fn syscall_arg(&self, idx: usize, _is_32bit: bool) -> usize {
    (match idx {
      0 => self.a0,
      1 => self.a1,
      2 => self.a2,
      3 => self.a3,
      4 => self.a4,
      5 => self.a5,
      _ => unimplemented!(),
    } as usize)
  }
}
