use std::ffi::CString;

use nix::{sys::ptrace, sys::ptrace::AddressType, unistd::Pid};

pub fn read_cstring(pid: Pid, address: AddressType) -> color_eyre::Result<CString> {
    let mut buf = Vec::new();
    let mut address = address;
    const WORD_SIZE: usize = 8; // FIXME
    loop {
        let Ok(word) = ptrace::read(pid, address) else { 
            log::warn!("Cannot read tracee {pid} memory {address:?}");
            return Ok(CString::new(buf)?)
        };
        let word_bytes = word.to_ne_bytes();
        for i in 0..WORD_SIZE {
            if word_bytes[i] == 0 {
                return Ok(CString::new(buf)?);
            }
            buf.push(word_bytes[i]);
        }
        address = unsafe { address.add(WORD_SIZE) };
    }
}

pub fn read_cstring_array(pid: Pid, mut address: AddressType) -> color_eyre::Result<Vec<CString>> {
    let mut res = Vec::new();
    const WORD_SIZE: usize = 8; // FIXME
    loop {
        let Ok(ptr) = ptrace::read(pid, address) else {
            log::warn!("Cannot read tracee {pid} memory {address:?}");
            return Ok(res)
        };
        if ptr == 0 {
            return Ok(res);
        } else {
            res.push(read_cstring(pid, ptr as AddressType)?);
        }
        address = unsafe { address.add(WORD_SIZE) };
    }
}
