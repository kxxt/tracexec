use std::{
  ffi::{CString, OsString},
  os::unix::prelude::OsStringExt,
  path::PathBuf,
};

use nix::{
  errno::Errno,
  sys::ptrace::{self, AddressType},
  unistd::Pid,
};
use tracing::warn;

pub type InspectError = Errno;

pub fn read_generic_string<TString>(
  pid: Pid,
  address: AddressType,
  ctor: impl Fn(Vec<u8>) -> TString,
) -> Result<TString, InspectError> {
  let mut buf = Vec::new();
  let mut address = address;
  const WORD_SIZE: usize = 8; // FIXME
  loop {
    let word = match ptrace::read(pid, address) {
      Err(e) => {
        warn!("Cannot read tracee {pid} memory {address:?}: {e}");
        return Err(e);
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
pub fn read_cstring(pid: Pid, address: AddressType) -> Result<CString, InspectError> {
  read_generic_string(pid, address, |x| CString::new(x).unwrap())
}

pub fn read_pathbuf(pid: Pid, address: AddressType) -> Result<PathBuf, InspectError> {
  read_generic_string(pid, address, |x| PathBuf::from(OsString::from_vec(x)))
}

pub fn read_string(pid: Pid, address: AddressType) -> Result<String, InspectError> {
  // Waiting on https://github.com/rust-lang/libs-team/issues/116
  read_generic_string(pid, address, |x| String::from_utf8_lossy(&x).into_owned())
}

pub fn read_null_ended_array<TItem>(
  pid: Pid,
  mut address: AddressType,
  reader: impl Fn(Pid, AddressType) -> Result<TItem, InspectError>,
) -> Result<Vec<TItem>, InspectError> {
  let mut res = Vec::new();
  const WORD_SIZE: usize = 8; // FIXME
  loop {
    let ptr = match ptrace::read(pid, address) {
      Err(e) => {
        warn!("Cannot read tracee {pid} memory {address:?}: {e}");
        return Err(e);
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
pub fn read_cstring_array(pid: Pid, address: AddressType) -> Result<Vec<CString>, InspectError> {
  read_null_ended_array(pid, address, read_cstring)
}

pub fn read_string_array(pid: Pid, address: AddressType) -> Result<Vec<String>, InspectError> {
  read_null_ended_array(pid, address, read_string)
}
