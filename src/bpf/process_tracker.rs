use std::collections::HashMap;

use nix::unistd::Pid;

#[derive(Default)]
pub struct ProcessTracker {
  processes: HashMap<Pid, ProcessState>,
}

#[derive(Debug, Default)]
pub struct ProcessState {
  associated_events: Vec<u64>,
}

impl ProcessTracker {
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

  pub fn associate_events(&mut self, pid: Pid, ids: impl IntoIterator<Item = u64>) {
    self
      .processes
      .get_mut(&pid)
      .unwrap()
      .associated_events
      .extend(ids);
  }

  pub fn force_associate_events(&mut self, pid: Pid, ids: impl IntoIterator<Item = u64>) {
    self
      .processes
      .entry(pid)
      .or_default()
      .associated_events
      .extend(ids);
  }

  #[allow(unused)]
  pub fn associated_events(&mut self, pid: Pid) -> &[u64] {
    &self.processes.get(&pid).unwrap().associated_events
  }

  pub fn maybe_associated_events(&mut self, pid: Pid) -> Option<&[u64]> {
    self
      .processes
      .get(&pid)
      .map(|p| p.associated_events.as_slice())
  }
}
