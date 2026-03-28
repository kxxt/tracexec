use hashbrown::HashMap;
use nix::unistd::Pid;
use tracexec_core::event::{
  EventId,
  ParentTracker,
};

#[derive(Default)]
pub struct ProcessTracker {
  processes: HashMap<Pid, ProcessState>,
}

#[derive(Debug, Default)]
pub struct ProcessState {
  associated_events: Vec<EventId>,
  /// # How parent tracking works in BPF
  ///
  /// ```text
  /// #                      A
  /// #                      │
  /// #             fork (0) │
  /// #            ◄─────────┤
  /// # - - - - - -│- - - - -│- - - - - - - - - - - - tracexec started
  /// #            │         │
  /// #  C  exec I │         │fork (1)
  /// #  ┌◄────────┘         └───────►
  /// #  │                           │
  /// #  │fork (2)                   │
  /// #  └────────┐                  │ exec II  B
  /// #           │exec III  D       └─────────►┐
  /// #           └──────────┐                  │
  /// #                      │           exec IV│
  /// #                      ▼        E ◄───────┘
  ///
  /// ```
  ///
  /// In system-wide tracing mode, the bpf tracer naturally misses all events
  /// happened before its start. So we will have multiple root events.
  ///
  /// When we process `fork (1)`, we will find out that the parent of the fork
  /// operation does not exist in our process tracker.
  ///
  /// So the resulting tree looks like this:
  ///
  /// - A spawns B
  ///   - B becomes E
  /// - A(fork) becomes C
  ///   - C spawns D
  pub parent_tracker: ParentTracker,
}

impl ProcessTracker {
  pub fn parent_tracker_disjoint_mut(
    &mut self,
    p1: Pid,
    p2: Pid,
  ) -> [Option<&mut ParentTracker>; 2] {
    self
      .processes
      .get_disjoint_mut([&p1, &p2])
      .map(|x| x.map(|y| &mut y.parent_tracker))
  }

  pub fn parent_tracker_mut(&mut self, pid: Pid) -> Option<&mut ParentTracker> {
    // TODO: bpf might experience from event loss. We probably want to insert a default entry if not found.
    self.processes.get_mut(&pid).map(|x| &mut x.parent_tracker)
  }

  pub fn contains(&self, pid: Pid) -> bool {
    self.processes.contains_key(&pid)
  }

  pub fn add(&mut self, pid: Pid) {
    let ret = self.processes.insert(pid, Default::default());
    assert!(ret.is_none())
  }

  pub fn remove(&mut self, pid: Pid) {
    let ret = self.processes.remove(&pid);
    assert!(ret.is_some())
  }

  pub fn maybe_remove(&mut self, pid: Pid) {
    let _ = self.processes.remove(&pid);
  }

  pub fn associate_events(&mut self, pid: Pid, ids: impl IntoIterator<Item = EventId>) {
    self
      .processes
      .get_mut(&pid)
      .unwrap()
      .associated_events
      .extend(ids);
  }

  pub fn force_associate_events(&mut self, pid: Pid, ids: impl IntoIterator<Item = EventId>) {
    self
      .processes
      .entry(pid)
      .or_default()
      .associated_events
      .extend(ids);
  }

  #[allow(unused)]
  pub fn associated_events(&self, pid: Pid) -> &[EventId] {
    &self.processes.get(&pid).unwrap().associated_events
  }

  pub fn maybe_associated_events(&self, pid: Pid) -> Option<&[EventId]> {
    self
      .processes
      .get(&pid)
      .map(|p| p.associated_events.as_slice())
  }
}

#[cfg(test)]
mod tests {
  use nix::unistd::Pid;
  use tracexec_core::event::EventId;

  use super::ProcessTracker;

  #[test]
  fn test_add_remove_and_associate_events() {
    let mut tracker = ProcessTracker::default();
    let pid = Pid::from_raw(123);
    tracker.add(pid);
    tracker.associate_events(pid, [EventId::new(1), EventId::new(2)]);
    let events = tracker.maybe_associated_events(pid).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], EventId::new(1));
    assert_eq!(events[1], EventId::new(2));
    tracker.remove(pid);
    assert!(tracker.maybe_associated_events(pid).is_none());
  }

  #[test]
  fn test_force_associate_events_inserts_missing_pid() {
    let mut tracker = ProcessTracker::default();
    let pid = Pid::from_raw(77);
    tracker.force_associate_events(pid, [EventId::new(9)]);
    let events = tracker.maybe_associated_events(pid).unwrap();
    assert_eq!(events, [EventId::new(9)]);
  }

  #[test]
  fn test_parent_tracker_disjoint_mut_for_two_pids() {
    let mut tracker = ProcessTracker::default();
    let p1 = Pid::from_raw(1);
    let p2 = Pid::from_raw(2);
    tracker.add(p1);
    tracker.add(p2);
    let [t1, t2] = tracker.parent_tracker_disjoint_mut(p1, p2);
    assert!(t1.is_some());
    assert!(t2.is_some());
  }

  #[test]
  fn test_maybe_remove_missing_pid_is_noop() {
    let mut tracker = ProcessTracker::default();
    tracker.maybe_remove(Pid::from_raw(999));
  }
}
