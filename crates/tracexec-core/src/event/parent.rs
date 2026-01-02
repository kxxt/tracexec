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
/// ```ignore
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
/// ```
///
/// To achieve this, we
/// 1) for `spawns`(A spawns B), record the id of last exec event(Unknown ?> A) of the parent process(A) at fork time.
/// 2) for `becomes`(C becomes D), record the id of last exec event(A becomes C)
///
/// If the process itself have successful execs, then the parent event is `last_successful_exec`
/// Otherwise, the parent is the corresponding successful exec event of its parent process.
#[derive(Debug, Clone, Default)]
pub struct ParentTracker {
  /// The parent event recorded at fork time,
  parent_last_exec: Option<EventId>,
  /// The last exec event of this process
  last_successful_exec: Option<EventId>,
}

impl ParentTracker {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn save_parent_last_exec(&mut self, parent: &Self) {
    self.parent_last_exec = parent.last_successful_exec.or(parent.parent_last_exec);
  }

  /// Updates parent tracker with an exec event
  /// and returns the parent event id of this exec event
  pub fn update_last_exec(&mut self, id: EventId, successful: bool) -> Option<ParentEventId> {
    let has_successful_exec = self.last_successful_exec.is_some();
    let old_last_exec = if successful {
      self.last_successful_exec.replace(id)
    } else {
      self.last_successful_exec
    };
    // If a process has successful exec events, the parent should be the last successful exec,
    // other wise it should point to the parent exec event
    if has_successful_exec {
      // This is at least the second time of exec for this process
      old_last_exec.map(ParentEvent::Become)
    } else {
      self.parent_last_exec.map(ParentEvent::Spawn)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parent_event_map() {
    let r#become: ParentEvent<u32> = ParentEvent::Become(10);
    let spawn: ParentEvent<u32> = ParentEvent::Spawn(20);

    let mapped_become = r#become.map(|x| x * 2);
    let mapped_spawn = spawn.map(|x| x + 5);

    match mapped_become {
      ParentEvent::Become(v) => assert_eq!(v, 20),
      _ => panic!("Expected Become variant"),
    }
    match mapped_spawn {
      ParentEvent::Spawn(v) => assert_eq!(v, 25),
      _ => panic!("Expected Spawn variant"),
    }
  }

  #[test]
  fn test_parent_event_transpose() {
    let become_some: ParentEvent<Option<u32>> = ParentEvent::Become(Some(10));
    let become_none: ParentEvent<Option<u32>> = ParentEvent::Become(None);
    let spawn_some: ParentEvent<Option<u32>> = ParentEvent::Spawn(Some(5));
    let spawn_none: ParentEvent<Option<u32>> = ParentEvent::Spawn(None);

    let transposed_become_some = become_some.transpose();
    let transposed_become_none = become_none.transpose();
    let transposed_spawn_some = spawn_some.transpose();
    let transposed_spawn_none = spawn_none.transpose();

    assert_eq!(transposed_become_some, Some(ParentEvent::Become(10)));
    assert_eq!(transposed_become_none, None);
    assert_eq!(transposed_spawn_some, Some(ParentEvent::Spawn(5)));
    assert_eq!(transposed_spawn_none, None);
  }

  #[test]
  fn test_parent_tracker_save_and_update() {
    let mut parent = ParentTracker::new();
    let mut child = ParentTracker::new();

    let parent_exec1 = EventId::new(1);
    let parent_exec2 = EventId::new(2);

    // First exec for parent
    assert_eq!(parent.update_last_exec(parent_exec1, true), None);
    // Second exec for parent
    let parent_become = parent.update_last_exec(parent_exec2, true);
    assert_eq!(parent_become.unwrap(), ParentEvent::Become(EventId::new(1)));

    // Save parent's last successful exec to child
    child.save_parent_last_exec(&parent);

    let child_exec = EventId::new(10);
    let parent_event = child.update_last_exec(child_exec, true);
    // First exec in child should reference parent's last exec as Spawn
    assert!(matches!(parent_event, Some(ParentEvent::Spawn(_))));
  }

  #[test]
  fn test_parent_tracker_update_unsuccessful_exec() {
    let mut tracker = ParentTracker::new();
    let parent_id = EventId::new(5);
    tracker.parent_last_exec = Some(parent_id);

    let exec_id = EventId::new(10);
    // unsuccessful exec does not update last_successful_exec
    let parent_event = tracker.update_last_exec(exec_id, false);
    assert!(matches!(parent_event, Some(ParentEvent::Spawn(_))));
    assert_eq!(tracker.last_successful_exec, None);
  }

  #[test]
  fn test_parent_tracker_multiple_execs() {
    let mut tracker = ParentTracker::new();
    let first_exec = EventId::new(1);
    let second_exec = EventId::new(2);

    // First successful exec
    let parent_event1 = tracker.update_last_exec(first_exec, true);
    assert!(parent_event1.is_none());
    assert_eq!(tracker.last_successful_exec.unwrap().into_inner(), 1);

    // Second successful exec
    let parent_event2 = tracker.update_last_exec(second_exec, true);
    // Should return Become of previous exec
    assert_eq!(parent_event2.unwrap(), ParentEvent::Become(EventId::new(1)));
    assert_eq!(tracker.last_successful_exec.unwrap().into_inner(), 2);
  }
}
