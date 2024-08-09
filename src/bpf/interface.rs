use nix::libc::pid_t;

#[derive(Debug)]
#[repr(C)]
pub struct StringEntryHeader {
  pub pid: pid_t,
  pub flags: u32,
  pub eid: u64,
}
