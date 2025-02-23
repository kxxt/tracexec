//! Common operations to run in tracee process

use std::{ffi::CString, os::fd::AsRawFd};

use nix::{
  errno::Errno,
  libc,
  unistd::{Gid, Uid, User, dup2, getpid, initgroups, setpgid, setresgid, setresuid, setsid},
};

pub fn nullify_stdio() -> Result<(), std::io::Error> {
  let dev_null = std::fs::File::open("/dev/null")?;
  dup2(dev_null.as_raw_fd(), 0)?;
  dup2(dev_null.as_raw_fd(), 1)?;
  dup2(dev_null.as_raw_fd(), 2)?;
  Ok(())
}

pub fn runas(user: &User, effective: Option<(Uid, Gid)>) -> Result<(), Errno> {
  let (euid, egid) = effective.unwrap_or((user.uid, user.gid));
  initgroups(&CString::new(user.name.as_str()).unwrap()[..], user.gid)?;
  setresgid(user.gid, egid, Gid::from_raw(u32::MAX))?;
  setresuid(user.uid, euid, Uid::from_raw(u32::MAX))?;
  Ok(())
}

pub fn lead_process_group() -> Result<(), Errno> {
  let me = getpid();
  setpgid(me, me)
}

pub fn lead_session_and_control_terminal() -> Result<(), Errno> {
  setsid()?;
  if unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) } == -1 {
    Err(Errno::last())?;
  }
  Ok(())
}
