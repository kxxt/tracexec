use std::ffi::CString;

use nix::unistd::Pid;

pub fn read_argv(pid: Pid) -> color_eyre::Result<Vec<CString>> {
    let filename = format!("/proc/{pid}/cmdline");
    let buf = std::fs::read(filename)?;
    Ok(buf
        .split(|&c| c == 0)
        .map(CString::new)
        .collect::<Result<Vec<_>, _>>()?)
}
