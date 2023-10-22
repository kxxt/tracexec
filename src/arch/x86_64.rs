macro_rules! syscall_no_from_regs {
    ($regs:expr) => {
        ($regs).orig_rax as i64
    };
}

macro_rules! syscall_res_from_regs {
    ($regs:expr) => {
        ($regs).rax as i64
    };
}

pub(crate) use syscall_no_from_regs;
pub(crate) use syscall_res_from_regs;
