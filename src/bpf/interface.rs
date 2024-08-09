use nix::libc::pid_t;

#[derive(Debug)]
#[repr(C)]
pub struct EventHeader {
  pub pid: pid_t,
  pub flags: u32,
  pub eid: u64,
  pub kind: EventType,
}

#[derive(Debug)]
#[repr(C)]
pub enum EventType {
  Sysenter,
  Sysexit,
  String,
}
