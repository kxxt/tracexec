macro_rules! syscall_no_from_regs {
    ($regs:ident) => {
        $regs.orig_rax as i64
    };
}

macro_rules! syscall_res_from_regs {
    ($regs:ident) => {
        $regs.rax as i64
    };
}

macro_rules! is_execveat_execve_quirk {
    ($regs:ident) => {
        $regs.rdi == 0 && $regs.rsi == 0 && $regs.rdx == 0
    };
}

macro_rules! syscall_arg {
    ($regs:ident, 0) => {
        $regs.rdi
    };
    ($regs:ident, 1) => {
        $regs.rsi
    };
    ($regs:ident, 2) => {
        $regs.rdx
    };
    ($regs:ident, 3) => {
        $regs.r10
    };
    ($regs:ident, 4) => {
        $regs.r8
    };
    ($regs:ident, 5) => {
        $regs.r9
    };
}

pub(crate) use is_execveat_execve_quirk;
pub(crate) use syscall_no_from_regs;
pub(crate) use syscall_res_from_regs;
pub(crate) use syscall_arg;
