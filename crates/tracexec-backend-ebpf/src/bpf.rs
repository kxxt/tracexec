use std::{
  borrow::Cow,
  ffi::CStr,
};

use tracexec_core::cache::{
  ArcStr,
  StringCache,
};
pub use tracexec_core::event::BpfError;

#[allow(
  clippy::large_stack_frames, // generated Default impl for large structs
  clippy::non_send_fields_in_send_ty,
  clippy::unwrap_used
 )]
pub mod skel {
  include!(concat!(env!("OUT_DIR"), "/tracexec_system.skel.rs"));
}

pub mod interface {
  include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/interface.rs"));
}

pub fn utf8_lossy_cow_from_bytes_with_nul(data: &[u8]) -> Cow<'_, str> {
  // SAFETY: We use this function to read c-string from BPF ringbuf memory.
  //         We manually ensure the presence of nul at BPF side.
  #[expect(clippy::unwrap_used)]
  String::from_utf8_lossy(CStr::from_bytes_until_nul(data).unwrap().to_bytes())
}

pub fn cached_cow(cow: Cow<str>) -> ArcStr {
  match cow {
    Cow::Borrowed(s) => CACHE.get_or_insert(s),
    Cow::Owned(s) => CACHE.get_or_insert_owned(s),
  }
}

static CACHE: StringCache = StringCache;

#[cfg(test)]
mod tests {
  use std::borrow::Cow;

  use super::*;

  #[test]
  fn test_utf8_lossy_cow_from_bytes_with_nul_stops_at_nul() {
    let input = b"hello\0ignored";
    let out = utf8_lossy_cow_from_bytes_with_nul(input);
    assert_eq!(out.as_ref(), "hello");
  }

  #[test]
  fn test_cached_cow_borrowed_and_owned() {
    let borrowed = cached_cow(Cow::Borrowed("alpha"));
    let owned = cached_cow(Cow::Owned("alpha".to_string()));
    assert_eq!(borrowed.as_ref(), "alpha");
    assert_eq!(owned.as_ref(), "alpha");
    // They should be pointing to the same cached instance
    assert_eq!(borrowed.as_ref().as_ptr(), owned.as_ref().as_ptr());
  }
}
