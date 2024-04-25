use nix::libc::user_regs_struct;

pub type PtraceRegisters = user_regs_struct;

macro_rules! syscall_no_from_regs {
  ($regs:ident) => {
    $regs.a7 as i64
  };
}

macro_rules! syscall_res_from_regs {
  ($regs:ident) => {
    $regs.a0 as i64
  };
}

macro_rules! syscall_arg {
  ($regs:ident, 0) => {
    $regs.a0
  };
  ($regs:ident, 1) => {
    $regs.a1
  };
  ($regs:ident, 2) => {
    $regs.a2
  };
  ($regs:ident, 3) => {
    $regs.a3
  };
  ($regs:ident, 4) => {
    $regs.a4
  };
  ($regs:ident, 5) => {
    $regs.a5
  };
}

pub(crate) use syscall_arg;
pub(crate) use syscall_no_from_regs;
pub(crate) use syscall_res_from_regs;
