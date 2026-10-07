use std::{
  alloc::{
    GlobalAlloc,
    Layout,
    System,
  },
  cell::Cell,
  hint::black_box,
};

use tracexec_core::cache::{
  ArcStr,
  StringCache,
};

struct CountingAllocator;

thread_local! {
  static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn record_allocation() {
  let _ = ALLOCATIONS.try_with(|count| {
    if let Some(value) = count.get() {
      count.set(Some(value + 1));
    }
  });
}

// SAFETY: every allocation and deallocation is forwarded unchanged to System.
// Thread-local counters keep unrelated test-harness allocations out of the measurement.
unsafe impl GlobalAlloc for CountingAllocator {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    record_allocation();
    unsafe { System.alloc(layout) }
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    record_allocation();
    unsafe { System.alloc_zeroed(layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    record_allocation();
    unsafe { System.realloc(ptr, layout, new_size) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { System.dealloc(ptr, layout) }
  }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn live_string_cache_hits_do_not_allocate() {
  let cache = StringCache::new();
  let value = cache.get_or_insert("a repeated environment value with enough bytes to allocate");
  let owned = value.to_string();

  ALLOCATIONS.set(Some(0));
  for _ in 0..1000 {
    black_box(cache.get_or_insert(black_box(value.as_str())));
    black_box(ArcStr::from(black_box(value.as_str())));
  }
  black_box(cache.get_or_insert_owned(owned));
  let allocations = ALLOCATIONS.replace(None);

  assert_eq!(allocations, Some(0));
}
