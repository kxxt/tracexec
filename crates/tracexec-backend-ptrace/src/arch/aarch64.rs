use nix::libc::user_regs_struct;

use super::RegsExt;

pub const NATIVE_AUDIT_ARCH: u32 = super::AUDIT_ARCH_AARCH64;
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
      0 => self.regs[0],
      1 => self.regs[1],
      2 => self.regs[2],
      3 => self.regs[3],
      4 => self.regs[4],
      5 => self.regs[5],
      _ => unimplemented!(),
    } as usize)
  }
}
