use std::{
    ffi::{CString, OsString},
    os::unix::prelude::OsStringExt,
    path::PathBuf,
};

use nix::{sys::ptrace, sys::ptrace::AddressType, unistd::Pid};

pub fn read_generic_string<TString>(
    pid: Pid,
    address: AddressType,
    ctor: impl Fn(Vec<u8>) -> TString,
) -> color_eyre::Result<TString> {
    let mut buf = Vec::new();
    let mut address = address;
    const WORD_SIZE: usize = 8; // FIXME
    loop {
        let word = match ptrace::read(pid, address) {
            Err(e) => {
                log::warn!("Cannot read tracee {pid} memory {address:?}: {e}");
                return Ok(ctor(buf));
            }
            Ok(word) => word,
        };
        let word_bytes = word.to_ne_bytes();
        for &byte in word_bytes.iter() {
            if byte == 0 {
                return Ok(ctor(buf));
            }
            buf.push(byte);
        }
        address = unsafe { address.add(WORD_SIZE) };
    }
}

#[allow(unused)]
pub fn read_cstring(pid: Pid, address: AddressType) -> color_eyre::Result<CString> {
    read_generic_string(pid, address, |x| CString::new(x).unwrap())
}

pub fn read_pathbuf(pid: Pid, address: AddressType) -> color_eyre::Result<PathBuf> {
    read_generic_string(pid, address, |x| PathBuf::from(OsString::from_vec(x)))
}

pub fn read_string(pid: Pid, address: AddressType) -> color_eyre::Result<String> {
    // Waiting on https://github.com/rust-lang/libs-team/issues/116
    read_generic_string(pid, address, |x| String::from_utf8_lossy(&x).to_string())
}

pub fn read_null_ended_array<TItem>(
    pid: Pid,
    mut address: AddressType,
    reader: impl Fn(Pid, AddressType) -> color_eyre::Result<TItem>,
) -> color_eyre::Result<Vec<TItem>> {
    let mut res = Vec::new();
    const WORD_SIZE: usize = 8; // FIXME
    loop {
        let ptr = match ptrace::read(pid, address) {
            Err(e) => {
                log::warn!("Cannot read tracee {pid} memory {address:?}: {e}");
                return Ok(res);
            }
            Ok(ptr) => ptr,
        };
        if ptr == 0 {
            return Ok(res);
        } else {
            res.push(reader(pid, ptr as AddressType)?);
        }
        address = unsafe { address.add(WORD_SIZE) };
    }
}

#[allow(unused)]
pub fn read_cstring_array(pid: Pid, address: AddressType) -> color_eyre::Result<Vec<CString>> {
    read_null_ended_array(pid, address, read_cstring)
}

pub fn read_string_array(pid: Pid, address: AddressType) -> color_eyre::Result<Vec<String>> {
    read_null_ended_array(pid, address, read_string)
}
