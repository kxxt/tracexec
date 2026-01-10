//! Common operations to run in tracee process

use std::{
  ffi::CString,
  os::fd::{AsFd, FromRawFd, OwnedFd},
};

use nix::{
  errno::Errno,
  libc,
  unistd::{Gid, Uid, User, dup2, getpid, initgroups, setpgid, setresgid, setresuid, setsid},
};

pub fn nullify_stdio() -> Result<(), std::io::Error> {
  let dev_null = std::fs::File::options()
    .read(true)
    .write(true)
    .open("/dev/null")?;
  let mut stdin = unsafe { OwnedFd::from_raw_fd(0) };
  let mut stdout = unsafe { OwnedFd::from_raw_fd(1) };
  let mut stderr = unsafe { OwnedFd::from_raw_fd(2) };
  dup2(dev_null.as_fd(), &mut stdin)?;
  dup2(dev_null.as_fd(), &mut stdout)?;
  dup2(dev_null.as_fd(), &mut stderr)?;
  std::mem::forget(stdin);
  std::mem::forget(stdout);
  std::mem::forget(stderr);
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

#[cfg(test)]
mod tests {
  use super::*;
  use nix::unistd::getpgrp;
  use rusty_fork::rusty_fork_test;
  use std::io::{Read, Write};

  rusty_fork_test! {
    #[test]
    fn test_nullify_stdio() {
      nullify_stdio().expect("nullify_stdio failed");

      // stdout should now point to /dev/null:
      // write should succeed
      let mut stdout = std::io::stdout();
      stdout.write_all(b"discarded").unwrap();
      stdout.flush().unwrap();

      // stdin should read EOF
      let mut buf = [0u8; 16];
      let mut stdin = std::io::stdin();
      let n = stdin.read(&mut buf).unwrap();
      assert_eq!(n, 0);
    }
  }

  rusty_fork_test! {
    #[test]
    fn test_lead_process_group() {
      let pid = nix::unistd::getpid();
      let pgrp_before = getpgrp();

      lead_process_group().expect("lead_process_group failed");

      let pgrp_after = getpgrp();

      // We should now be our own process group leader
      assert_eq!(pgrp_after, pid);

      // Ensure we actually changed if not already leader
      let _ = pgrp_before;
    }
  }
}
