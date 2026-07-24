//! Simple implementation of a `!Send` & `!Sync` channel.
//!
//! It is useful for single threaded scenarios.
//!
//! Specifically, in tracexec it is used to implement the event-action pattern:
//! https://ratatui.rs/concepts/event-handling/#centralized-catching-message-passing

use std::{
  cell::RefCell,
  collections::VecDeque,
  rc::Rc,
};

pub fn unbounded<T>() -> (LocalUnboundedSender<T>, LocalUnboundedReceiver<T>) {
  let inner = Rc::new(RefCell::new(VecDeque::new()));
  (
    LocalUnboundedSender {
      inner: inner.clone(),
    },
    LocalUnboundedReceiver { inner },
  )
}

#[derive(Debug, Clone)]
pub struct LocalUnboundedSender<T> {
  inner: Rc<RefCell<VecDeque<T>>>,
}

#[derive(Debug, Clone)]
pub struct LocalUnboundedReceiver<T> {
  inner: Rc<RefCell<VecDeque<T>>>,
}

impl<T> LocalUnboundedSender<T> {
  pub fn send(&self, v: T) {
    self.inner.borrow_mut().push_back(v);
  }
}
impl<T> LocalUnboundedReceiver<T> {
  pub fn receive(&self) -> Option<T> {
    self.inner.borrow_mut().pop_front()
  }
}

#[cfg(test)]
mod tests {
  use test_that::prelude::*;

  use super::*;

  #[test]
  fn test_basic_send_receive() {
    let (sender, receiver) = unbounded();

    sender.send(42);
    sender.send(100);

    assert_that!(receiver.receive(), some(eq(42)));
    assert_that!(receiver.receive(), some(eq(100)));
    // Queue is now empty
    assert_that!(receiver.receive(), none());
  }

  #[test]
  fn test_receive_empty_channel() {
    let (_sender, receiver): (LocalUnboundedSender<i32>, _) = unbounded();
    assert_that!(receiver.receive(), none());
  }

  #[test]
  fn test_multiple_senders_receive() {
    let (sender1, receiver1) = unbounded();
    let sender2 = sender1.clone();
    let receiver2 = receiver1.clone();

    sender1.send(1);
    sender2.send(2);

    // Order is preserved
    assert_that!(receiver1.receive(), some(eq(1)));
    assert_that!(receiver2.receive(), some(eq(2)));

    // Channel empty now
    assert_that!(receiver1.receive(), none());
    assert_that!(receiver2.receive(), none());
  }

  #[test]
  fn test_fifo_order_with_multiple_elements() {
    let (sender, receiver) = unbounded();

    for i in 0..10 {
      sender.send(i);
    }

    for i in 0..10 {
      assert_that!(receiver.receive(), some(eq(i)));
    }

    assert_that!(receiver.receive(), none());
  }

  #[test]
  fn test_clone_sender_receiver_shares_queue() {
    let (sender, receiver) = unbounded();
    let sender2 = sender.clone();
    let receiver2 = receiver.clone();

    sender.send(1);
    sender2.send(2);

    assert_that!(receiver2.receive(), some(eq(1)));
    assert_that!(receiver.receive(), some(eq(2)));
    assert_that!(receiver2.receive(), none());
  }
}
