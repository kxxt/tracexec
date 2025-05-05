//! Code for locating the id of parent event of an event.

use super::EventId;
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentEvent<T> {
  /// The parent process destroys itself and become a new process
  Become(T),
  /// The parent process spawns a new process.
  Spawn(T),
}

impl From<ParentEvent<Self>> for EventId {
  fn from(value: ParentEvent<Self>) -> Self {
    match value {
      ParentEvent::Become(event_id) | ParentEvent::Spawn(event_id) => event_id,
    }
  }
}

impl<T> ParentEvent<T> {
  pub fn map<U>(self, f: impl FnOnce(T) -> U) -> ParentEvent<U> {
    match self {
      Self::Become(v) => ParentEvent::Become(f(v)),
      Self::Spawn(v) => ParentEvent::Spawn(f(v)),
    }
  }
}

impl<T> ParentEvent<Option<T>> {
  pub fn transpose(self) -> Option<ParentEvent<T>> {
    match self {
      Self::Become(v) => v.map(ParentEvent::Become),
      Self::Spawn(v) => v.map(ParentEvent::Spawn),
    }
  }
}

pub type ParentEventId = ParentEvent<EventId>;

/// How this works
///
/// Consider the following two situations:
///
///           pid 2
///          Proc A
///            │  fork   pid 3
///  pid 2     ├────────►Proc A
/// Proc C exec│           │      pid 3
///   ┌───◄────┘           │exec Proc B
///   │        *           └───►────┐
///   │*********                    │
///   │ alt exec                    │
/// C exec Proc D
///
/// We will derive the following relations:
///
/// Unknown ?> A
/// |- A spawns B
/// |- A becomes C
///    |- C becomes D
///
/// To achieve this, we
/// 1) for `spawns`(A spawns B), record the id of last exec event(Unknown ?> A) of the parent process(A) at fork time.
/// 2) for `becomes`(C becomes D), record the id of last exec event(A becomes C)
///
/// If the exec_count of a process after a exec is equal or greater than 2, then the parent event is `last_exec`
/// If the exec_count of a process after a exec is 1, then the parent event is `parent_last_exec`
#[derive(Debug, Clone, Default)]
pub struct ParentTracker {
  /// How many times do the process exec.
  ///
  /// We only need to check if it occurs more than once.
  successful_exec_count: u8,
  /// The parent event recorded at fork time,
  parent_last_exec: Option<EventId>,
  /// The last exec event of this process
  last_exec: Option<EventId>,
}

impl ParentTracker {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn save_parent_last_exec(&mut self, parent: &Self) {
    self.parent_last_exec = parent.last_exec.or(parent.parent_last_exec);
  }

  /// Updates parent tracker with an exec event
  /// and returns the parent event id of this exec event
  pub fn update_last_exec(&mut self, id: EventId, successful: bool) -> Option<ParentEventId> {
    let old_last_exec = if successful {
      self.successful_exec_count += 1;
      self.last_exec.replace(id)
    } else {
      self.last_exec
    };
    if self.successful_exec_count >= 2 {
      // This is at least the second time of exec for this process
      old_last_exec.map(ParentEvent::Become)
    } else {
      self.parent_last_exec.map(ParentEvent::Spawn)
    }
  }
}
