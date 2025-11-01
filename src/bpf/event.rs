use std::collections::HashMap;

use nix::libc::gid_t;

use crate::{
  event::OutputMsg,
  proc::{CredInspectError, FileDescriptorInfoCollection, cached_string},
};

/// The temporary storage for receiving information
/// about an event from the BPF ringbuf
#[derive(Debug)]
pub struct EventStorage {
  pub strings: Vec<OutputMsg>,
  pub fdinfo_map: FileDescriptorInfoCollection,
  pub paths: HashMap<i32, Path>,
  pub groups: Result<Vec<gid_t>, CredInspectError>,
}

impl Default for EventStorage {
  fn default() -> Self {
    Self {
      strings: Default::default(),
      fdinfo_map: Default::default(),
      paths: Default::default(),
      groups: Err(CredInspectError::Inspect),
    }
  }
}

#[derive(Debug, Default, Clone)]
pub struct Path {
  // Used to avoid prefixing
  // paths from synthetic filesystems
  // with /
  pub is_absolute: bool,
  pub segments: Vec<OutputMsg>,
}

impl From<Path> for OutputMsg {
  fn from(value: Path) -> Self {
    let mut s = String::with_capacity(
      value
        .segments
        .iter()
        .map(|s| s.as_ref().len())
        .sum::<usize>()
        + value.segments.len(),
    );
    if value.is_absolute {
      s.push('/');
    }
    let mut error = false;
    for (idx, segment) in value.segments.iter().enumerate().rev() {
      if segment.not_ok() {
        error = true;
      }
      s.push_str(segment.as_ref());
      if idx != 0 {
        s.push('/');
      }
    }
    (if error {
      OutputMsg::PartialOk
    } else {
      OutputMsg::Ok
    })(cached_string(s))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum BpfError {
  Dropped,
  Flags,
}
