use std::{
  borrow::Cow,
  ffi::CStr,
  sync::{LazyLock, RwLock},
};

use tracexec_core::cache::{ArcStr, StringCache};

pub use tracexec_core::event::BpfError;
pub mod event;
pub mod process_tracker;
pub mod tracer;

#[allow(clippy::use_self)] // remove after https://github.com/libbpf/libbpf-rs/pull/1231 is merged
pub mod skel {
  include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/tracexec_system.skel.rs"
  ));
}

pub mod interface {
  include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bpf/interface.rs"));
}

fn utf8_lossy_cow_from_bytes_with_nul(data: &[u8]) -> Cow<'_, str> {
  String::from_utf8_lossy(CStr::from_bytes_until_nul(data).unwrap().to_bytes())
}

fn cached_cow(cow: Cow<str>) -> ArcStr {
  match cow {
    Cow::Borrowed(s) => CACHE.write().unwrap().get_or_insert(s),
    Cow::Owned(s) => CACHE.write().unwrap().get_or_insert_owned(s),
  }
}

static CACHE: LazyLock<RwLock<StringCache>> = LazyLock::new(|| RwLock::new(StringCache::new()));
