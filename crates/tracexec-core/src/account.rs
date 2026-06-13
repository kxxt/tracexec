//! Account database helpers.
//!
//! Static glibc binaries cannot safely use NSS-backed account lookups such as
//! `getpwuid_r(3)`. In that build mode we read the local account files
//! directly; other builds keep the usual libc/NSS behavior.

#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
use std::ffi::CString;
use std::path::PathBuf;
#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
use std::{
  os::unix::ffi::OsStrExt,
  path::Path,
};

#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
use nix::errno::Errno;
use nix::unistd::{
  Gid,
  Group,
  Uid,
  User,
};

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
const ETC_PASSWD: &str = "/etc/passwd";
#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
const ETC_GROUP: &str = "/etc/group";

#[cfg(not(all(target_env = "gnu", target_feature = "crt-static")))]
pub fn user_from_uid(uid: Uid) -> nix::Result<Option<User>> {
  User::from_uid(uid)
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
pub fn user_from_uid(uid: Uid) -> nix::Result<Option<User>> {
  find_user_in_passwd(&read_account_file(ETC_PASSWD)?, |user| user.uid == uid)
}

#[cfg(not(all(target_env = "gnu", target_feature = "crt-static")))]
pub fn user_from_name(name: &str) -> nix::Result<Option<User>> {
  User::from_name(name)
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
pub fn user_from_name(name: &str) -> nix::Result<Option<User>> {
  find_user_in_passwd(&read_account_file(ETC_PASSWD)?, |user| user.name == name)
}

#[cfg(not(all(target_env = "gnu", target_feature = "crt-static")))]
pub fn group_from_gid(gid: Gid) -> nix::Result<Option<Group>> {
  Group::from_gid(gid)
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
pub fn group_from_gid(gid: Gid) -> nix::Result<Option<Group>> {
  find_group_in_group(&read_account_file(ETC_GROUP)?, |group| group.gid == gid)
}

pub fn current_shell() -> nix::Result<Option<PathBuf>> {
  user_from_uid(nix::unistd::getuid()).map(|user| user.map(|user| user.shell))
}

/// Parse `/etc/group` content to find supplementary group IDs for a user.
///
/// Returns a deduplicated list of GIDs including the primary GID.
#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
pub fn parse_supplementary_gids(
  etc_group_content: &str,
  username: &str,
  primary_gid: Gid,
) -> Vec<Gid> {
  let mut gids = vec![primary_gid];
  for line in etc_group_content.lines() {
    let Some(group) = parse_group_line(line) else {
      continue;
    };
    if group.mem.iter().any(|member| member == username) && !gids.contains(&group.gid) {
      gids.push(group.gid);
    }
  }
  gids
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
pub fn supplementary_gids(username: &str, primary_gid: Gid) -> nix::Result<Vec<Gid>> {
  Ok(parse_supplementary_gids(
    &read_account_file(ETC_GROUP)?,
    username,
    primary_gid,
  ))
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
fn read_account_file(path: &str) -> nix::Result<String> {
  std::fs::read_to_string(path).map_err(io_error_to_errno)
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
fn io_error_to_errno(err: std::io::Error) -> Errno {
  err.raw_os_error().map_or(Errno::EIO, Errno::from_raw)
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
fn find_user_in_passwd(
  content: &str,
  predicate: impl Fn(&User) -> bool,
) -> nix::Result<Option<User>> {
  for line in content.lines() {
    let Some(user) = parse_passwd_line(line)? else {
      continue;
    };
    if predicate(&user) {
      return Ok(Some(user));
    }
  }
  Ok(None)
}

#[cfg(all(target_env = "gnu", target_feature = "crt-static"))]
fn find_group_in_group(
  content: &str,
  predicate: impl Fn(&Group) -> bool,
) -> nix::Result<Option<Group>> {
  for line in content.lines() {
    let Some(group) = parse_group_line(line) else {
      continue;
    };
    if predicate(&group) {
      return Ok(Some(group));
    }
  }
  Ok(None)
}

#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
fn parse_passwd_line(line: &str) -> nix::Result<Option<User>> {
  let line = line.trim();
  if line.is_empty() || line.starts_with('#') {
    return Ok(None);
  }

  let mut fields = line.split(':');
  let Some(name) = fields.next() else {
    return Ok(None);
  };
  let Some(passwd) = fields.next() else {
    return Ok(None);
  };
  let Some(uid) = fields.next().and_then(|uid| uid.parse::<u32>().ok()) else {
    return Ok(None);
  };
  let Some(gid) = fields.next().and_then(|gid| gid.parse::<u32>().ok()) else {
    return Ok(None);
  };
  let Some(gecos) = fields.next() else {
    return Ok(None);
  };
  let Some(dir) = fields.next() else {
    return Ok(None);
  };
  let Some(shell) = fields.next() else {
    return Ok(None);
  };
  if fields.next().is_some() {
    return Ok(None);
  }

  Ok(Some(User {
    name: name.to_owned(),
    passwd: cstring(passwd)?,
    uid: Uid::from_raw(uid),
    gid: Gid::from_raw(gid),
    gecos: cstring(gecos)?,
    dir: pathbuf_from_bytes(dir),
    shell: pathbuf_from_bytes(shell),
  }))
}

#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
fn parse_group_line(line: &str) -> Option<Group> {
  let line = line.trim();
  if line.is_empty() || line.starts_with('#') {
    return None;
  }

  let mut fields = line.split(':');
  let name = fields.next()?;
  let passwd = fields.next()?;
  let gid = fields.next()?.parse::<u32>().ok()?;
  let members = fields.next().unwrap_or("");
  if fields.next().is_some() {
    return None;
  }

  Some(Group {
    name: name.to_owned(),
    passwd: CString::new(passwd).ok()?,
    gid: Gid::from_raw(gid),
    mem: members
      .split(',')
      .map(str::trim)
      .filter(|member| !member.is_empty())
      .map(str::to_owned)
      .collect(),
  })
}

#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
fn cstring(s: &str) -> nix::Result<CString> {
  CString::new(s).map_err(|_| Errno::EINVAL)
}

#[cfg(any(test, all(target_env = "gnu", target_feature = "crt-static")))]
fn pathbuf_from_bytes(s: &str) -> PathBuf {
  Path::new(std::ffi::OsStr::from_bytes(s.as_bytes())).to_path_buf()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_passwd_line_basic() {
    let user = parse_passwd_line("alice:x:1000:100:Alice:/home/alice:/bin/bash")
      .unwrap()
      .unwrap();
    assert_eq!(user.name, "alice");
    assert_eq!(user.uid, Uid::from_raw(1000));
    assert_eq!(user.gid, Gid::from_raw(100));
    assert_eq!(user.shell, PathBuf::from("/bin/bash"));
  }

  #[test]
  fn test_parse_passwd_line_skips_malformed_lines() {
    assert!(parse_passwd_line("# comment").unwrap().is_none());
    assert!(parse_passwd_line("missing:fields").unwrap().is_none());
    assert!(
      parse_passwd_line("alice:x:not-a-uid:100:Alice:/home/alice:/bin/bash")
        .unwrap()
        .is_none()
    );
    assert!(
      parse_passwd_line("alice:x:1000:100:Alice:/home/alice:/bin/bash:extra")
        .unwrap()
        .is_none()
    );
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
    let content = "nogroup:x:65534\n";
    let gids = parse_supplementary_gids(content, "alice", Gid::from_raw(1000));
    assert_eq!(gids, vec![Gid::from_raw(1000)]);
  }
}
