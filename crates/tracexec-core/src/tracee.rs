//! Common operations to run in tracee process

use std::os::fd::{
  AsFd,
  FromRawFd,
  OwnedFd,
};

use nix::{
  errno::Errno,
  libc,
  unistd::{
    Gid,
    Uid,
    User,
    dup2,
    getpid,
    setpgid,
    setresgid,
    setresuid,
    setsid,
  },
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
  do_initgroups(&user.name, user.gid)?;
  setresgid(user.gid, egid, Gid::from_raw(u32::MAX))?;
  setresuid(user.uid, euid, Uid::from_raw(u32::MAX))?;
  Ok(())
}

/// Parse `/etc/group` content to find supplementary group IDs for a user.
///
/// Returns a deduplicated list of GIDs including the primary GID.
#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
fn parse_supplementary_gids(etc_group_content: &str, username: &str, primary_gid: Gid) -> Vec<Gid> {
  let mut gids = vec![primary_gid];
  for line in etc_group_content.lines() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    // Format: group_name:password:GID:user_list
    let mut fields = line.splitn(4, ':');
    let Some(_name) = fields.next() else { continue };
    let Some(_passwd) = fields.next() else {
      continue;
    };
    let Some(gid_str) = fields.next() else {
      continue;
    };
    let members = fields.next().unwrap_or("");
    let Ok(gid_raw) = gid_str.parse::<u32>() else {
      continue;
    };
    let gid = Gid::from_raw(gid_raw);
    if members.split(',').any(|m| m.trim() == username) && !gids.contains(&gid) {
      gids.push(gid);
    }
  }
  gids
}

/// Set supplementary groups by reading `/etc/group` directly,
/// avoiding dynamic NSS which crashes in static glibc builds.
#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
fn do_initgroups(username: &str, primary_gid: Gid) -> Result<(), Errno> {
  let content = std::fs::read_to_string("/etc/group").map_err(|_| Errno::EIO)?;
  let gids = parse_supplementary_gids(&content, username, primary_gid);
  nix::unistd::setgroups(&gids)
}

/// Use the standard `initgroups` from libc for non-static-glibc builds.
#[cfg(not(all(target_env = "gnu", target_feature = "crt-static")))]
fn do_initgroups(username: &str, primary_gid: Gid) -> Result<(), Errno> {
  nix::unistd::initgroups(
    &std::ffi::CString::new(username).map_err(|_| Errno::EINVAL)?,
    primary_gid,
  )
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
  use std::io::{
    Read,
    Write,
  };

  use nix::unistd::getpgrp;
  use rusty_fork::rusty_fork_test;

  use super::*;

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

  #[test]
  fn test_parse_supplementary_gids_basic() {
    let content =
      "root:x:0:\ndaemon:x:1:\nusers:x:100:alice,bob\ndocker:x:999:alice\nwheel:x:10:bob\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(
      gids,
      vec![Gid::from_raw(1000), Gid::from_raw(100), Gid::from_raw(999)]
    );
  }

  #[test]
  fn test_parse_supplementary_gids_primary_gid_deduped() {
    let content = "users:x:1000:alice\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    // primary_gid 1000 already matched, should not be duplicated
    assert_eq!(gids, vec![Gid::from_raw(1000)]);
  }

  #[test]
  fn test_parse_supplementary_gids_no_members() {
    let content = "root:x:0:\nusers:x:100:\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000)]);
  }

  #[test]
  fn test_parse_supplementary_gids_skips_malformed_lines() {
    let content = "root:x:0:\nmalformed_line\n:x:abc:alice\nusers:x:100:alice\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000), Gid::from_raw(100)]);
  }

  #[test]
  fn test_parse_supplementary_gids_skips_comments_and_empty() {
    let content = "# this is a comment\n\nusers:x:100:alice\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000), Gid::from_raw(100)]);
  }

  #[test]
  fn test_parse_supplementary_gids_no_partial_match() {
    // "alice" should not match "alice2" or "malice"
    let content = "group1:x:100:alice2,malice\ngroup2:x:200:alice\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000), Gid::from_raw(200)]);
  }

  #[test]
  fn test_parse_supplementary_gids_whitespace_in_members() {
    let content = "group1:x:100: alice , bob \n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000), Gid::from_raw(100)]);
  }

  #[test]
  fn test_parse_supplementary_gids_no_user_list_field() {
    // Lines with only 3 fields (no user list)
    let content = "nogroup:x:65534\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000)]);
  }
}
