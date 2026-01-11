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
  /// ```
  ///                      A
  ///                      │
  ///             fork (0) │
  ///            ◄─────────┤
  /// - - - - - -│- - - - -│- - - - - - - - - - - - tracexec started
  ///            │         │
  ///  C  exec I │         │fork (1)
  ///  ┌◄────────┘         └───────►
  ///  │                           │
  ///  │fork (2)                   │
  ///  └────────┐                  │ exec II  B
  ///           │exec III  D       └─────────►┐
  ///           └──────────┐                  │
  ///                      │           exec IV│
  ///                      ▼        E ◄───────┘
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
