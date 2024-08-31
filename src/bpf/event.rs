use std::collections::HashMap;

use arcstr::ArcStr;

use crate::{event::OutputMsg, proc::FileDescriptorInfoCollection};

/// The temporary storage for receiving information
/// about an event from the BPF ringbuf
#[derive(Debug, Default)]
pub struct EventStorage {
  pub strings: Vec<(ArcStr, u32)>,
  pub fdinfo_map: FileDescriptorInfoCollection,
  pub paths: HashMap<i32, Path>,
}

#[derive(Debug)]
pub struct Path {
  // Used to avoid prefixing
  // paths from synthetic filesystems
  // with /
  is_absolute: bool,
  segments: Vec<OutputMsg>,
}
