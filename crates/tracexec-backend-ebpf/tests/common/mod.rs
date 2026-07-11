use std::{
  sync::{
    Arc,
    Mutex,
  },
  time::{
    Duration,
    Instant,
  },
};

use libbpf_rs::RingBuffer;

pub struct EventSlot<T> {
  event: Arc<Mutex<Option<T>>>,
}

impl<T> Clone for EventSlot<T> {
  fn clone(&self) -> Self {
    Self {
      event: Arc::clone(&self.event),
    }
  }
}

impl<T: Copy> EventSlot<T> {
  pub fn new() -> Self {
    Self {
      event: Arc::new(Mutex::new(None)),
    }
  }

  pub fn store_matching(&self, data: &[u8], predicate: impl FnOnce(&T) -> bool) {
    if data.len() != std::mem::size_of::<T>() {
      return;
    }

    // SAFETY: eBPF event structs used by these tests are plain old data
    // produced by the BPF program, and ringbuf samples are 8 byte aligned.
    let event = unsafe { std::ptr::read(data.as_ptr() as *const T) };
    if predicate(&event) {
      *self.event.lock().unwrap() = Some(event);
    }
  }

  pub fn wait(
    &self,
    rb: &RingBuffer<'_>,
    timeout: Duration,
    missing_message: &str,
  ) -> color_eyre::Result<T> {
    let start = Instant::now();
    while start.elapsed() < timeout {
      rb.poll(Duration::from_millis(50))?;
      if self.event.lock().unwrap().is_some() {
        break;
      }
    }

    Ok(self.event.lock().unwrap().expect(missing_message))
  }
}
