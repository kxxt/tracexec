use std::{
  borrow::Cow,
  ffi::CStr,
};

use tracexec_core::cache::{
  ArcStr,
  StringCache,
};
pub use tracexec_core::event::BpfError;
pub mod event;
pub mod process_tracker;
pub mod tracer;

#[allow(
  clippy::use_self, // remove after https://github.com/libbpf/libbpf-rs/pull/1231 landed
  clippy::large_stack_frames, // generated Default impl for large structs
  clippy::non_send_fields_in_send_ty
 )]
pub mod skel {
  include!(concat!(env!("OUT_DIR"), "/tracexec_system.skel.rs"));
}

pub mod interface {
  include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bpf/interface.rs"));
}

fn utf8_lossy_cow_from_bytes_with_nul(data: &[u8]) -> Cow<'_, str> {
  String::from_utf8_lossy(CStr::from_bytes_until_nul(data).unwrap().to_bytes())
}

fn cached_cow(cow: Cow<str>) -> ArcStr {
  match cow {
    Cow::Borrowed(s) => CACHE.get_or_insert(s),
    Cow::Owned(s) => CACHE.get_or_insert_owned(s),
  }
}

static CACHE: StringCache = StringCache;
