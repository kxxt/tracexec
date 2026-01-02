//! This module provides utilities about processing process information(e.g. comm, argv, envp).

use core::fmt;
use std::{
  borrow::Cow,
  collections::{BTreeMap, BTreeSet, HashSet},
  ffi::CString,
  fmt::{Display, Formatter},
  fs,
  io::{self, BufRead, BufReader, Read},
  os::raw::c_int,
  path::{Path, PathBuf},
};

use crate::cache::ArcStr;
use filedescriptor::AsRawFileDescriptor;
use owo_colors::OwoColorize;

use nix::{
  fcntl::OFlag,
  libc::{AT_FDCWD, gid_t},
  unistd::{Pid, getpid},
};
use serde::{Serialize, Serializer, ser::SerializeSeq};
use snafu::Snafu;
use tracing::warn;

use crate::{cache::StringCache, event::OutputMsg, pty::UnixSlavePty};

#[allow(unused)]
pub fn read_argv(pid: Pid) -> color_eyre::Result<Vec<CString>> {
  let filename = format!("/proc/{pid}/cmdline");
  let buf = std::fs::read(filename)?;
  Ok(
    buf
      .split(|&c| c == 0)
      .map(CString::new)
      .collect::<Result<Vec<_>, _>>()?,
  )
}

pub fn read_comm(pid: Pid) -> color_eyre::Result<ArcStr> {
  let filename = format!("/proc/{pid}/comm");
  let mut buf = std::fs::read(filename)?;
  buf.pop(); // remove trailing newline
  let utf8 = String::from_utf8_lossy(&buf);
  Ok(CACHE.get_or_insert(&utf8))
}

pub fn read_cwd(pid: Pid) -> std::io::Result<ArcStr> {
  let filename = format!("/proc/{pid}/cwd");
  let buf = std::fs::read_link(filename)?;
  Ok(cached_str(&buf.to_string_lossy()))
}

pub fn read_exe(pid: Pid) -> std::io::Result<ArcStr> {
  let filename = format!("/proc/{pid}/exe");
  let buf = std::fs::read_link(filename)?;
  Ok(cached_str(&buf.to_string_lossy()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStatus {
  pub cred: Cred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cred {
  pub groups: Vec<gid_t>,
  pub uid_real: u32,
  pub uid_effective: u32,
  pub uid_saved_set: u32,
  pub uid_fs: u32,
  pub gid_real: u32,
  pub gid_effective: u32,
  pub gid_saved_set: u32,
  pub gid_fs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Snafu)]
pub enum CredInspectError {
  #[snafu(display("Failed to read credential info: {kind}"))]
  Io { kind: std::io::ErrorKind },
  #[snafu(display("Failed to inspect credential info from kernel"))]
  Inspect,
}

pub fn read_status(pid: Pid) -> std::io::Result<ProcStatus> {
  let filename = format!("/proc/{pid}/status");
  let contents = fs::read_to_string(filename)?;
  parse_status_contents(&contents)
}

fn parse_status_contents(contents: &str) -> std::io::Result<ProcStatus> {
  let mut uid = None;
  let mut gid = None;
  let mut groups = None;

  fn parse_ids(s: &str) -> std::io::Result<[u32; 4]> {
    let mut iter = s.trim_ascii().split_ascii_whitespace().take(4).map(|v| {
      v.parse()
        .map_err(|_| std::io::Error::new(io::ErrorKind::InvalidData, "non numeric uid/gid"))
    });
    Ok([
      iter
        .next()
        .transpose()?
        .ok_or_else(|| std::io::Error::new(io::ErrorKind::InvalidData, "not enough uid/gid(s)"))?,
      iter
        .next()
        .transpose()?
        .ok_or_else(|| std::io::Error::new(io::ErrorKind::InvalidData, "not enough uid/gid(s)"))?,
      iter
        .next()
        .transpose()?
        .ok_or_else(|| std::io::Error::new(io::ErrorKind::InvalidData, "not enough uid/gid(s)"))?,
      iter
        .next()
        .transpose()?
        .ok_or_else(|| std::io::Error::new(io::ErrorKind::InvalidData, "not enough uid/gid(s)"))?,
    ])
  }

  for line in contents.lines() {
    if let Some(rest) = line.strip_prefix("Uid:") {
      uid = Some(parse_ids(rest)?);
    } else if let Some(rest) = line.strip_prefix("Gid:") {
      gid = Some(parse_ids(rest)?);
    } else if let Some(rest) = line.strip_prefix("Groups:") {
      let r: Result<Vec<_>, _> = rest
        .trim_ascii()
        .split_ascii_whitespace()
        .map(|v| {
          v.parse()
            .map_err(|_| std::io::Error::new(io::ErrorKind::InvalidData, "non numeric group id"))
        })
        .collect();
      groups = Some(r?);
    }

    if uid.is_some() && gid.is_some() && groups.is_some() {
      break;
    }
  }

  let Some([uid_real, uid_effective, uid_saved_set, uid_fs]) = uid else {
    return Err(std::io::Error::new(
      io::ErrorKind::InvalidData,
      "status output does not contain uids",
    ));
  };
  let Some([gid_real, gid_effective, gid_saved_set, gid_fs]) = gid else {
    return Err(std::io::Error::new(
      io::ErrorKind::InvalidData,
      "status output does not contain gids",
    ));
  };
  let Some(groups) = groups else {
    return Err(std::io::Error::new(
      io::ErrorKind::InvalidData,
      "status output does not contain groups",
    ));
  };

  Ok(ProcStatus {
    cred: Cred {
      groups,
      uid_real,
      uid_effective,
      uid_saved_set,
      uid_fs,
      gid_real,
      gid_effective,
      gid_saved_set,
      gid_fs,
    },
  })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FileDescriptorInfoCollection {
  #[serde(flatten)]
  pub fdinfo: BTreeMap<c_int, FileDescriptorInfo>,
}

impl FileDescriptorInfoCollection {
  pub fn stdin(&self) -> Option<&FileDescriptorInfo> {
    self.fdinfo.get(&0)
  }

  pub fn stdout(&self) -> Option<&FileDescriptorInfo> {
    self.fdinfo.get(&1)
  }

  pub fn stderr(&self) -> Option<&FileDescriptorInfo> {
    self.fdinfo.get(&2)
  }

  pub fn get(&self, fd: c_int) -> Option<&FileDescriptorInfo> {
    self.fdinfo.get(&fd)
  }

  pub fn new_baseline() -> color_eyre::Result<Self> {
    let mut fdinfo = BTreeMap::new();
    let pid = getpid();
    fdinfo.insert(0, read_fdinfo(pid, 0)?);
    fdinfo.insert(1, read_fdinfo(pid, 1)?);
    fdinfo.insert(2, read_fdinfo(pid, 2)?);

    Ok(Self { fdinfo })
  }

  pub fn with_pts(pts: &UnixSlavePty) -> color_eyre::Result<Self> {
    let mut result = Self::default();
    let ptyfd = &pts.fd;
    let raw_fd = ptyfd.as_raw_file_descriptor();
    let mut info = read_fdinfo(getpid(), raw_fd)?;
    for fd in 0..3 {
      info.fd = fd;
      result.fdinfo.insert(fd, read_fdinfo(getpid(), raw_fd)?);
    }
    Ok(result)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDescriptorInfo {
  pub fd: c_int,
  pub path: OutputMsg,
  pub pos: usize,
  #[serde(serialize_with = "serialize_oflags")]
  pub flags: OFlag,
  pub mnt_id: c_int,
  pub ino: u64,
  pub mnt: ArcStr,
  pub extra: Vec<ArcStr>,
}

impl FileDescriptorInfo {
  pub fn not_same_file_as(&self, other: &Self) -> bool {
    !self.same_file_as(other)
  }

  pub fn same_file_as(&self, other: &Self) -> bool {
    self.ino == other.ino && self.mnt_id == other.mnt_id
  }
}

fn serialize_oflags<S>(oflag: &OFlag, serializer: S) -> Result<S::Ok, S::Error>
where
  S: Serializer,
{
  let mut seq = serializer.serialize_seq(None)?;
  let mut flag_display = String::with_capacity(16);
  for f in oflag.iter() {
    flag_display.clear();
    bitflags::parser::to_writer(&f, &mut flag_display).unwrap();
    seq.serialize_element(&flag_display)?;
  }
  seq.end()
}

impl Default for FileDescriptorInfo {
  fn default() -> Self {
    Self {
      fd: Default::default(),
      path: OutputMsg::Ok(ArcStr::default()),
      pos: Default::default(),
      flags: OFlag::empty(),
      mnt_id: Default::default(),
      ino: Default::default(),
      mnt: Default::default(),
      extra: Default::default(),
    }
  }
}

pub fn read_fd(pid: Pid, fd: i32) -> std::io::Result<ArcStr> {
  if fd == AT_FDCWD {
    return read_cwd(pid);
  }
  let filename = format!("/proc/{pid}/fd/{fd}");
  Ok(cached_str(&std::fs::read_link(filename)?.to_string_lossy()))
}

/// Read /proc/{pid}/fdinfo/{fd} to get more information about the file descriptor.
pub fn read_fdinfo(pid: Pid, fd: i32) -> color_eyre::Result<FileDescriptorInfo> {
  let filename = format!("/proc/{pid}/fdinfo/{fd}");
  let file = std::fs::File::open(filename)?;
  let reader = BufReader::new(file);
  let mut info = FileDescriptorInfo::default();
  for line in reader.lines() {
    let line = line?;
    let mut parts = line.split_ascii_whitespace();
    let key = parts.next().unwrap_or("");
    let value = parts.next().unwrap_or("");
    match key {
      "pos:" => info.pos = value.parse()?,
      "flags:" => info.flags = OFlag::from_bits_truncate(c_int::from_str_radix(value, 8)?),
      "mnt_id:" => info.mnt_id = value.parse()?,
      "ino:" => info.ino = value.parse()?,
      _ => {
        let line = CACHE.get_or_insert_owned(line);
        info.extra.push(line)
      }
    }
  }
  info.mnt = get_mountinfo_by_mnt_id(pid, info.mnt_id)?;
  info.path = read_fd(pid, fd).map(OutputMsg::Ok)?;
  Ok(info)
}

pub fn read_fds(pid: Pid) -> color_eyre::Result<FileDescriptorInfoCollection> {
  let mut collection = FileDescriptorInfoCollection::default();
  let filename = format!("/proc/{pid}/fdinfo");
  for entry in std::fs::read_dir(filename)? {
    let entry = entry?;
    let fd = entry.file_name().to_string_lossy().parse()?;
    collection.fdinfo.insert(fd, read_fdinfo(pid, fd)?);
  }
  Ok(collection)
}

fn get_mountinfo_by_mnt_id(pid: Pid, mnt_id: c_int) -> color_eyre::Result<ArcStr> {
  let filename = format!("/proc/{pid}/mountinfo");
  let file = std::fs::File::open(filename)?;
  let reader = BufReader::new(file);
  for line in reader.lines() {
    let line = line?;
    let parts = line.split_once(' ');
    if parts.map(|(mount_id, _)| mount_id.parse()) == Some(Ok(mnt_id)) {
      return Ok(CACHE.get_or_insert_owned(line));
    }
  }
  Ok(CACHE.get_or_insert("Not found. This is probably a pipe or something else."))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "what", content = "value", rename_all = "kebab-case")]
pub enum Interpreter {
  None,
  Shebang(ArcStr),
  ExecutableInaccessible,
  Error(ArcStr),
}

impl Display for Interpreter {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::None => write!(f, "{}", "none".bold()),
      Self::Shebang(s) => write!(f, "{s:?}"),
      Self::ExecutableInaccessible => {
        write!(f, "{}", "executable inaccessible".red().bold())
      }
      Self::Error(e) => write!(f, "({}: {})", "err".red().bold(), e.red().bold()),
    }
  }
}

pub fn read_interpreter_recursive(exe: impl AsRef<Path>) -> Vec<Interpreter> {
  let mut exe = Cow::Borrowed(exe.as_ref());
  let mut interpreters = Vec::new();
  loop {
    match read_interpreter(exe.as_ref()) {
      Interpreter::Shebang(shebang) => {
        exe = Cow::Owned(PathBuf::from(
          shebang.split_ascii_whitespace().next().unwrap_or(""),
        ));
        interpreters.push(Interpreter::Shebang(shebang));
      }
      Interpreter::None => break,
      err => {
        interpreters.push(err);
        break;
      }
    };
  }
  interpreters
}

pub fn read_interpreter(exe: &Path) -> Interpreter {
  fn err_to_interpreter(e: io::Error) -> Interpreter {
    if e.kind() == io::ErrorKind::PermissionDenied || e.kind() == io::ErrorKind::NotFound {
      Interpreter::ExecutableInaccessible
    } else {
      let e = CACHE.get_or_insert_owned(e.to_string());
      Interpreter::Error(e)
    }
  }
  let file = match std::fs::File::open(exe) {
    Ok(file) => file,
    Err(e) => return err_to_interpreter(e),
  };
  let mut reader = BufReader::new(file);
  // First, check if it's a shebang script
  let mut buf = [0u8; 2];

  if let Err(e) = reader.read_exact(&mut buf) {
    if e.kind() == std::io::ErrorKind::UnexpectedEof {
      // File is too short to contain a shebang
      return Interpreter::None;
    }
    let e = CACHE.get_or_insert_owned(e.to_string());
    return Interpreter::Error(e);
  };
  if &buf != b"#!" {
    return Interpreter::None;
  }
  // Read the rest of the line
  let mut buf = Vec::new();

  if let Err(e) = reader.read_until(b'\n', &mut buf) {
    let e = CACHE.get_or_insert_owned(e.to_string());
    return Interpreter::Error(e);
  };
  // Get trimmed shebang line [start, end) indices
  // If the shebang line is empty, we don't care
  let start = buf
    .iter()
    .position(|&c| !c.is_ascii_whitespace())
    .unwrap_or(0);
  let end = buf
    .iter()
    .rposition(|&c| !c.is_ascii_whitespace())
    .map(|x| x + 1)
    .unwrap_or(buf.len());
  let shebang = String::from_utf8_lossy(&buf[start..end]);
  let shebang = CACHE.get_or_insert(&shebang);
  Interpreter::Shebang(shebang)
}

pub fn parse_env_entry(item: &str) -> (&str, &str) {
  // trace!("Parsing envp entry: {:?}", item);
  let Some(mut sep_loc) = item.as_bytes().iter().position(|&x| x == b'=') else {
    warn!(
      "Invalid envp entry: {:?}, assuming value to empty string!",
      item
    );
    return (item, "");
  };
  if sep_loc == 0 {
    // Find the next equal sign
    sep_loc = item
      .as_bytes()
      .iter()
      .skip(1)
      .position(|&x| x == b'=')
      .unwrap_or_else(|| {
        warn!(
          "Invalid envp entry starting with '=': {:?}, assuming value to empty string!",
          item
        );
        item.len()
      });
  }
  let (head, tail) = item.split_at(sep_loc);
  (head, { if tail.is_empty() { "" } else { &tail[1..] } })
}

pub fn parse_failiable_envp(envp: Vec<OutputMsg>) -> (BTreeMap<OutputMsg, OutputMsg>, bool) {
  let mut has_dash_var = false;
  (
    envp
      .into_iter()
      .map(|entry| {
        if let OutputMsg::Ok(s) | OutputMsg::PartialOk(s) = entry {
          let (key, value) = parse_env_entry(&s);
          if key.starts_with('-') {
            has_dash_var = true;
          }
          (
            OutputMsg::Ok(CACHE.get_or_insert(key)),
            OutputMsg::Ok(CACHE.get_or_insert(value)),
          )
        } else {
          (entry.clone(), entry)
        }
      })
      .collect(),
    has_dash_var,
  )
}

pub fn cached_str(s: &str) -> ArcStr {
  CACHE.get_or_insert(s)
}

pub fn cached_string(s: String) -> ArcStr {
  CACHE.get_or_insert_owned(s)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvDiff {
  has_added_or_modified_keys_starting_with_dash: bool,
  pub added: BTreeMap<OutputMsg, OutputMsg>,
  pub removed: BTreeSet<OutputMsg>,
  pub modified: BTreeMap<OutputMsg, OutputMsg>,
}

impl EnvDiff {
  pub fn is_modified_or_removed(&self, key: &OutputMsg) -> bool {
    self.modified.contains_key(key) || self.removed.contains(key)
  }

  /// Whether we need to use `--` to prevent argument injection
  pub fn need_env_argument_separator(&self) -> bool {
    self.has_added_or_modified_keys_starting_with_dash
  }
}

pub fn diff_env(
  original: &BTreeMap<OutputMsg, OutputMsg>,
  envp: &BTreeMap<OutputMsg, OutputMsg>,
) -> EnvDiff {
  let mut added = BTreeMap::new();
  let mut modified = BTreeMap::<OutputMsg, OutputMsg>::new();
  // Use str to avoid cloning all env vars
  let mut removed: HashSet<OutputMsg> = original.keys().cloned().collect();
  let mut has_added_or_modified_keys_starting_with_dash = false;
  for (key, value) in envp.iter() {
    // Too bad that we still don't have if- and while-let-chains
    // https://github.com/rust-lang/rust/issues/53667
    if let Some(orig_v) = original.get(key) {
      if orig_v != value {
        modified.insert(key.clone(), value.clone());
        if key.as_ref().starts_with('-') {
          has_added_or_modified_keys_starting_with_dash = true;
        }
      }
      removed.remove(key);
    } else {
      added.insert(key.clone(), value.clone());
      if key.as_ref().starts_with('-') {
        has_added_or_modified_keys_starting_with_dash = true;
      }
    }
  }
  EnvDiff {
    has_added_or_modified_keys_starting_with_dash,
    added,
    removed: removed.into_iter().collect(),
    modified,
  }
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineInfo {
  pub cwd: OutputMsg,
  pub env: BTreeMap<OutputMsg, OutputMsg>,
  pub fdinfo: FileDescriptorInfoCollection,
}

impl BaselineInfo {
  pub fn new() -> color_eyre::Result<Self> {
    let cwd = cached_str(&std::env::current_dir()?.to_string_lossy()).into();
    let env = std::env::vars()
      .map(|(k, v)| {
        (
          CACHE.get_or_insert_owned(k).into(),
          CACHE.get_or_insert_owned(v).into(),
        )
      })
      .collect();
    let fdinfo = FileDescriptorInfoCollection::new_baseline()?;
    Ok(Self { cwd, env, fdinfo })
  }

  pub fn with_pts(pts: &UnixSlavePty) -> color_eyre::Result<Self> {
    let cwd = cached_str(&std::env::current_dir()?.to_string_lossy()).into();
    let env = std::env::vars()
      .map(|(k, v)| {
        (
          CACHE.get_or_insert_owned(k).into(),
          CACHE.get_or_insert_owned(v).into(),
        )
      })
      .collect();
    let fdinfo = FileDescriptorInfoCollection::with_pts(pts)?;
    Ok(Self { cwd, env, fdinfo })
  }
}

static CACHE: StringCache = StringCache;

#[cfg(test)]
mod proc_status_tests {
  use super::*;

  #[test]
  fn test_parse_status_contents_valid() {
    let sample = "\
Name:\ttestproc
State:\tR (running)
Uid:\t1000\t1001\t1002\t1003
Gid:\t2000\t2001\t2002\t2003
Threads:\t1
Groups:\t0\t1\t2
";

    let status = parse_status_contents(sample).unwrap();
    assert_eq!(
      status,
      ProcStatus {
        cred: Cred {
          groups: vec![0, 1, 2],
          uid_real: 1000,
          uid_effective: 1001,
          uid_saved_set: 1002,
          uid_fs: 1003,
          gid_real: 2000,
          gid_effective: 2001,
          gid_saved_set: 2002,
          gid_fs: 2003,
        }
      }
    );
  }

  #[test]
  fn test_parse_status_contents_missing_gid() {
    let sample = "Uid:\t1\t2\t3\t4\nGroups:\t0\n";
    let e = parse_status_contents(sample).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
  }

  #[test]
  fn test_parse_status_contents_missing_groups() {
    let sample = "Uid:\t1\t2\t3\t4\nGid:\t0\t1\t2\t3\n";
    let e = parse_status_contents(sample).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
  }

  #[test]
  fn test_parse_status_contents_non_numeric_uid() {
    let sample = "\
Uid:\ta\t2\t3\t4
Gid:\t1\t2\t3\t4
Groups:\t0
";
    let err = parse_status_contents(sample).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
  }

  #[test]
  fn test_parse_status_contents_not_enough_uids() {
    let sample = "\
Uid:\t1\t2
Gid:\t1\t2\t3\t4
Groups:\t0
";
    let err = parse_status_contents(sample).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
  }
}

#[cfg(test)]
mod env_tests {
  use crate::event::FriendlyError;

  use super::*;

  #[test]
  fn test_parse_env_entry_normal() {
    let (k, v) = parse_env_entry("KEY=value");
    assert_eq!(k, "KEY");
    assert_eq!(v, "value");
  }

  #[test]
  fn test_parse_env_entry_missing_equal() {
    let (k, v) = parse_env_entry("KEY");
    assert_eq!(k, "KEY");
    assert_eq!(v, "");
  }

  #[test]
  fn test_parse_env_entry_leading_equal() {
    let (k, v) = parse_env_entry("=value");
    assert_eq!(k, "=value");
    assert_eq!(v, "");
  }

  #[test]
  fn test_parse_env_entry_multiple_equals() {
    let (k, v) = parse_env_entry("A=B=C");
    assert_eq!(k, "A");
    assert_eq!(v, "B=C");
  }

  #[test]
  fn test_parse_failiable_envp_basic() {
    let envp = vec![OutputMsg::Ok("A=1".into()), OutputMsg::Ok("B=2".into())];

    let (map, has_dash) = parse_failiable_envp(envp);

    assert!(!has_dash);
    assert_eq!(map.len(), 2);
    assert_eq!(
      map.get(&OutputMsg::Ok("A".into())).unwrap(),
      &OutputMsg::Ok("1".into())
    );
  }

  #[test]
  fn test_parse_failiable_envp_dash_key() {
    let envp = vec![OutputMsg::Ok("-X=1".into())];

    let (_map, has_dash) = parse_failiable_envp(envp);

    assert!(has_dash);
  }

  #[test]
  fn test_parse_failiable_envp_error_passthrough() {
    let envp = vec![OutputMsg::Err(FriendlyError::InspectError(
      nix::errno::Errno::EAGAIN,
    ))];

    let (map, _) = parse_failiable_envp(envp);

    assert!(matches!(
      map.values().next().unwrap(),
      OutputMsg::Err(FriendlyError::InspectError(nix::errno::Errno::EAGAIN))
    ));
  }
}

#[cfg(test)]
mod env_diff_tests {
  use std::collections::BTreeMap;

  use crate::{event::OutputMsg, proc::diff_env};

  #[test]
  fn test_env_diff_added_removed_modified() {
    let orig = BTreeMap::from([
      (OutputMsg::Ok("A".into()), OutputMsg::Ok("1".into())),
      (OutputMsg::Ok("B".into()), OutputMsg::Ok("2".into())),
    ]);

    let new = BTreeMap::from([
      (OutputMsg::Ok("A".into()), OutputMsg::Ok("10".into())),
      (OutputMsg::Ok("C".into()), OutputMsg::Ok("3".into())),
    ]);

    let diff = diff_env(&orig, &new);

    assert_eq!(diff.modified.len(), 1);
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.removed.len(), 1);

    assert!(diff.modified.contains_key(&OutputMsg::Ok("A".into())));
    assert!(diff.added.contains_key(&OutputMsg::Ok("C".into())));
    assert!(diff.removed.contains(&OutputMsg::Ok("B".into())));
  }

  #[test]
  fn test_env_diff_dash_key_requires_separator() {
    let orig = BTreeMap::new();
    let new = BTreeMap::from([(
      OutputMsg::Ok("-LD_PRELOAD".into()),
      OutputMsg::Ok("evil.so".into()),
    )]);

    let diff = diff_env(&orig, &new);

    assert!(diff.need_env_argument_separator());
  }
}

#[cfg(test)]
mod fdinfo_tests {
  use crate::proc::FileDescriptorInfo;

  #[test]
  fn test_fdinfo_same_file() {
    let a = FileDescriptorInfo {
      ino: 1,
      mnt_id: 2,
      ..Default::default()
    };

    let b = FileDescriptorInfo {
      ino: 1,
      mnt_id: 2,
      ..Default::default()
    };

    assert!(a.same_file_as(&b));
    assert!(!a.not_same_file_as(&b));
  }

  #[test]
  fn test_fdinfo_not_same_file() {
    let a = FileDescriptorInfo {
      ino: 1,
      mnt_id: 2,
      ..Default::default()
    };

    let b = FileDescriptorInfo {
      ino: 3,
      mnt_id: 2,
      ..Default::default()
    };

    assert!(a.not_same_file_as(&b));
  }
}

#[cfg(test)]
mod interpreter_test {
  use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::PermissionsExt,
  };

  use tempfile::tempdir;

  use crate::proc::{Interpreter, cached_str, read_interpreter, read_interpreter_recursive};

  #[test]
  fn test_interpreter_display() {
    let none = Interpreter::None;
    assert!(none.to_string().contains("none"));

    let err = Interpreter::Error(cached_str("boom"));
    assert!(err.to_string().contains("err"));
  }

  #[test]
  fn test_read_interpreter_none() {
    let dir = tempdir().unwrap();
    let exe = dir.path().join("binary");
    File::create(&exe).unwrap();

    let result = read_interpreter(&exe);
    assert_eq!(result, Interpreter::None);
    dir.close().unwrap();
  }

  #[test]
  fn test_read_interpreter_shebang() {
    let dir = tempdir().unwrap();

    let target = dir.path().join("target");
    File::create(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();

    let script = dir.path().join("script");
    let mut f = File::create(&script).unwrap();
    writeln!(f, "#!{}", target.display()).unwrap();

    let result = read_interpreter(&script);
    match result {
      Interpreter::Shebang(s) => assert!(s.as_ref().ends_with("target")),
      other => panic!("unexpected result: {other:?}"),
    }
    dir.close().unwrap();
  }

  #[test]
  fn test_read_interpreter_inaccessible() {
    let dir = tempdir().unwrap();
    let exe = dir.path().join("noaccess");
    File::create(&exe).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o000)).unwrap();

    let result = read_interpreter(&exe);
    assert_eq!(result, Interpreter::ExecutableInaccessible);
    dir.close().unwrap();
  }

  #[test]
  fn test_read_interpreter_empty_file() {
    let dir = tempdir().unwrap();
    let exe = dir.path().join("empty");
    File::create(&exe).unwrap();

    let result = read_interpreter(&exe);
    assert_eq!(result, Interpreter::None);
    dir.close().unwrap();
  }

  #[test]
  fn test_read_interpreter_recursive_shebang_chain() {
    use super::read_interpreter_recursive;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();

    // interpreter2: real binary (no shebang)
    // Note: an edge case that the file length does not permit it to contain shebang thus EOF.
    let interp2 = dir.path().join("interp2");
    File::create(&interp2).unwrap();
    fs::set_permissions(&interp2, fs::Permissions::from_mode(0o755)).unwrap();

    // interpreter1: shebang -> interpreter2
    let interp1 = dir.path().join("interp1");
    {
      let mut f = File::create(&interp1).unwrap();
      writeln!(f, "#!{}", interp2.display()).unwrap();
      f.flush().unwrap();
    }
    fs::set_permissions(&interp1, fs::Permissions::from_mode(0o755)).unwrap();

    // script: shebang -> interpreter1
    let script = dir.path().join("script");
    {
      let mut f = File::create(&script).unwrap();
      writeln!(f, "#!{}", interp1.display()).unwrap();
      f.flush().unwrap();
    }
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let result = read_interpreter_recursive(&script);

    assert_eq!(result.len(), 2);

    match &result[0] {
      Interpreter::Shebang(s) => {
        assert!(s.as_ref().ends_with("interp1"));
      }
      other => panic!("unexpected interpreter: {other:?}"),
    }

    match &result[1] {
      Interpreter::Shebang(s) => {
        assert!(s.as_ref().ends_with("interp2"));
      }
      other => panic!("unexpected interpreter: {other:?}"),
    }

    dir.close().unwrap();
  }

  #[test]
  fn test_read_interpreter_recursive_no_shebang() {
    use std::fs::File;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let exe = dir.path().join("binary");
    File::create(&exe).unwrap();

    let result = read_interpreter_recursive(&exe);
    assert!(result.is_empty());
    dir.close().unwrap();
  }
}
