use nix::libc::user_regs_struct;

pub const SYS_EXECVE_32: i32 = nix::libc::ENOSYS;
pub const SYS_EXECVEAT_32: i32 = nix::libc::ENOSYS;

pub type PtraceRegisters = user_regs_struct;

macro_rules! syscall_no_from_regs {
  ($regs:ident) => {
    $regs.regs[8] as i64
  };
}

macro_rules! syscall_res_from_regs {
  ($regs:ident) => {
    $regs.regs[0] as i64
  };
}

macro_rules! syscall_arg {
  ($regs:ident, 0) => {
    $regs.regs[0]
  };
  ($regs:ident, 1) => {
    $regs.regs[1]
  };
  ($regs:ident, 2) => {
    $regs.regs[2]
  };
  ($regs:ident, 3) => {
    $regs.regs[3]
  };
  ($regs:ident, 4) => {
    $regs.regs[4]
  };
  ($regs:ident, 5) => {
    $regs.regs[5]
  };
}

pub(crate) use syscall_arg;
pub(crate) use syscall_no_from_regs;
pub(crate) use syscall_res_from_regs;
