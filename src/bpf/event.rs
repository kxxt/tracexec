use std::collections::HashMap;

use arcstr::ArcStr;

use crate::{event::OutputMsg, proc::FileDescriptorInfoCollection};

/// The temporary storage for receiving information
/// about an event from the BPF ringbuf
pub struct EventStorage {
  strings: Vec<(ArcStr, u32)>,
  fdinfo_map: FileDescriptorInfoCollection,
  pathes: HashMap<i32, Path>,
}

pub struct Path {
  // Used to avoid prefixing
  // paths from synthetic filesystems
  // with /
  is_absolute: bool,
  segments: Vec<OutputMsg>,
}
