use std::{
  collections::BTreeMap,
  ffi::CString,
};

use nix::{
  sys::ptrace::{
    self,
    AddressType,
  },
  unistd::Pid,
};
use tracexec_core::{
  cache::ArcStr,
  event::OutputMsg,
  proc::{
    cached_str,
    cached_string,
    parse_env_entry,
  },
  tracer::InspectError,
};
use tracing::warn;

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

pub fn read_arcstr(pid: Pid, address: AddressType) -> Result<ArcStr, InspectError> {
  read_generic_string(pid, address, |x| cached_str(&String::from_utf8_lossy(&x)))
}

pub fn read_string(pid: Pid, address: AddressType) -> Result<String, InspectError> {
  // Waiting on https://github.com/rust-lang/libs-team/issues/116
  read_generic_string(pid, address, |x| String::from_utf8_lossy(&x).into_owned())
}

pub fn read_null_ended_array<TItem>(
  pid: Pid,
  mut address: AddressType,
  is_32bit: bool,
  reader: impl Fn(Pid, AddressType) -> Result<TItem, InspectError>,
) -> Result<Vec<TItem>, InspectError> {
  let mut res = Vec::new();
  // FIXME: alignment
  let word_size = if is_32bit { 4 } else { 8 };
  loop {
    let ptr = match ptrace::read(pid, address) {
      Err(e) => {
        warn!("Cannot read tracee {pid} memory {address:?}: {e}");
        return Err(e);
      }
      Ok(ptr) => {
        if is_32bit {
          ptr as u32 as i64
        } else {
          ptr
        }
      }
    };
    if ptr == 0 {
      return Ok(res);
    } else {
      res.push(reader(pid, ptr as AddressType)?);
    }
    address = unsafe { address.add(word_size) };
  }
}

#[allow(unused)]
pub fn read_cstring_array(
  pid: Pid,
  address: AddressType,
  is_32bit: bool,
) -> Result<Vec<CString>, InspectError> {
  read_null_ended_array(pid, address, is_32bit, read_cstring)
}

#[allow(unused)]
pub fn read_string_array(
  pid: Pid,
  address: AddressType,
  is_32bit: bool,
) -> Result<Vec<String>, InspectError> {
  read_null_ended_array(pid, address, is_32bit, read_string)
}

#[allow(unused)]
pub fn read_arcstr_array(
  pid: Pid,
  address: AddressType,
  is_32bit: bool,
) -> Result<Vec<ArcStr>, InspectError> {
  read_null_ended_array(pid, address, is_32bit, |pid, address| {
    read_string(pid, address).map(cached_string)
  })
}

pub fn read_output_msg_array(
  pid: Pid,
  address: AddressType,
  is_32bit: bool,
) -> Result<Vec<OutputMsg>, InspectError> {
  read_null_ended_array(pid, address, is_32bit, |pid, address| {
    read_string(pid, address)
      .map(cached_string)
      .map(OutputMsg::Ok)
  })
}

fn read_single_env_entry(pid: Pid, address: AddressType) -> (OutputMsg, OutputMsg) {
  let result = read_generic_string(pid, address, |bytes| {
    let utf8 = String::from_utf8_lossy(&bytes);
    let (k, v) = parse_env_entry(&utf8);
    let k = cached_str(k);
    let v = cached_str(v);
    (k, v)
  });
  match result {
    Ok((k, v)) => (OutputMsg::Ok(k), OutputMsg::Ok(v)),
    Err(e) => {
      let err = OutputMsg::Err(tracexec_core::event::FriendlyError::InspectError(e));
      (err.clone(), err)
    }
  }
}

pub fn read_env(
  pid: Pid,
  mut address: AddressType,
  is_32bit: bool,
) -> Result<(bool, BTreeMap<OutputMsg, OutputMsg>), InspectError> {
  let mut res = BTreeMap::new();
  let mut has_dash_env = false;
  // FIXME: alignment
  let word_size = if is_32bit { 4 } else { 8 };
  loop {
    let ptr = match ptrace::read(pid, address) {
      Err(e) => {
        warn!("Cannot read tracee {pid} memory {address:?}: {e}");
        return Err(e);
      }
      Ok(ptr) => {
        if is_32bit {
          ptr as u32 as i64
        } else {
          ptr
        }
      }
    };
    if ptr == 0 {
      return Ok((has_dash_env, res));
    } else {
      let (k, v) = read_single_env_entry(pid, ptr as AddressType);
      if k.as_ref().starts_with('-') {
        has_dash_env = true;
      }
      res.insert(k, v);
    }
    address = unsafe { address.add(word_size) };
  }
}
