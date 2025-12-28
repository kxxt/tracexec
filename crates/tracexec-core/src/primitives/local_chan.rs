//! Simple implementation of a `!Send` & `!Sync` channel.
//!
//! It is useful for single threaded scenarios.
//!
//! Specifically, in tracexec it is used to implement the event-action pattern:
//! https://ratatui.rs/concepts/event-handling/#centralized-catching-message-passing

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

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
