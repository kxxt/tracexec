//! This module provides utilities about processing process information(e.g. comm, argv, envp).

use core::fmt;
use std::{
  borrow::Cow,
  collections::{BTreeMap, BTreeSet, HashSet},
  ffi::CString,
  fmt::{Display, Formatter},
  io::{self, BufRead, BufReader, Read},
  os::raw::c_int,
  path::{Path, PathBuf},
};

use filedescriptor::AsRawFileDescriptor;
use owo_colors::OwoColorize;

use nix::{
  fcntl::OFlag,
  libc::AT_FDCWD,
  unistd::{getpid, Pid},
};

use crate::pty::UnixSlavePty;

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

pub fn read_comm(pid: Pid) -> color_eyre::Result<String> {
  let filename = format!("/proc/{pid}/comm");
  let mut buf = std::fs::read(filename)?;
  buf.pop(); // remove trailing newline
  Ok(String::from_utf8(buf)?)
}

pub fn read_cwd(pid: Pid) -> std::io::Result<PathBuf> {
  let filename = format!("/proc/{pid}/cwd");
  let buf = std::fs::read_link(filename)?;
  Ok(buf)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileDescriptorInfoCollection {
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

#[derive(Debug, Clone, PartialEq)]
pub struct FileDescriptorInfo {
  pub fd: c_int,
  pub path: PathBuf,
  pub pos: usize,
  pub flags: OFlag,
  pub mnt_id: c_int,
  pub ino: c_int,
  pub mnt: String,
  pub extra: Vec<String>,
}

impl Default for FileDescriptorInfo {
  fn default() -> Self {
    Self {
      fd: Default::default(),
      path: Default::default(),
      pos: Default::default(),
      flags: OFlag::empty(),
      mnt_id: Default::default(),
      ino: Default::default(),
      mnt: Default::default(),
      extra: Default::default(),
    }
  }
}

pub fn read_fd(pid: Pid, fd: i32) -> std::io::Result<PathBuf> {
  if fd == AT_FDCWD {
    return read_cwd(pid);
  }
  let filename = format!("/proc/{pid}/fd/{fd}");
  std::fs::read_link(filename)
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
      _ => info.extra.push(line),
    }
  }
  info.path = read_fd(pid, fd)?;
  info.mnt = get_mountinfo_by_mnt_id(pid, info.mnt_id)?;
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

pub fn get_mountinfo_by_mnt_id(pid: Pid, mnt_id: c_int) -> color_eyre::Result<String> {
  let filename = format!("/proc/{pid}/mountinfo");
  let file = std::fs::File::open(filename)?;
  let reader = BufReader::new(file);
  for line in reader.lines() {
    let line = line?;
    let parts = line.split_once(|x| x == ' ');
    if parts.map(|(mount_id, _)| mount_id.parse()) == Some(Ok(mnt_id)) {
      return Ok(line);
    }
  }
  Ok("Not found. This is probably a pipe or something else.".to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Interpreter {
  None,
  Shebang(String),
  ExecutableUnaccessible,
  Error(String),
}

impl Display for Interpreter {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Interpreter::None => write!(f, "{}", "none".bold()),
      Interpreter::Shebang(s) => write!(f, "{:?}", s),
      Interpreter::ExecutableUnaccessible => {
        write!(f, "{}", "executable unaccessible".red().bold())
      }
      Interpreter::Error(e) => write!(f, "({}: {})", "err".red().bold(), e.red().bold()),
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
      Interpreter::ExecutableUnaccessible
    } else {
      Interpreter::Error(e.to_string())
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
    return Interpreter::Error(e.to_string());
  };
  if &buf != b"#!" {
    return Interpreter::None;
  }
  // Read the rest of the line
  let mut buf = Vec::new();

  if let Err(e) = reader.read_until(b'\n', &mut buf) {
    return Interpreter::Error(e.to_string());
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
  Interpreter::Shebang(shebang.into_owned())
}

pub fn parse_env_entry(item: &str) -> (&str, &str) {
  log::trace!("Parsing envp entry: {:?}", item);
  let Some(mut sep_loc) = item.as_bytes().iter().position(|&x| x == b'=') else {
    log::warn!(
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
        log::warn!(
          "Invalid envp entry starting with '=': {:?}, assuming value to empty string!",
          item
        );
        item.len()
      });
  }
  let (head, tail) = item.split_at(sep_loc);
  (head, &tail[1..])
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnvDiff {
  pub added: BTreeMap<String, String>,
  pub removed: BTreeSet<String>,
  pub modified: BTreeMap<String, String>,
}

impl EnvDiff {
  pub fn is_modified_or_removed(&self, key: &str) -> bool {
    self.modified.contains_key(key) || self.removed.contains(key)
  }
}

pub fn diff_env(original: &BTreeMap<String, String>, envp: &[String]) -> EnvDiff {
  let mut added = BTreeMap::new();
  let mut modified = BTreeMap::new();
  // Use str to avoid cloning all env vars
  let mut removed: HashSet<&str> = original.keys().map(|v| v.as_str()).collect();
  for entry in envp.iter() {
    let (key, value) = parse_env_entry(entry);
    // Too bad that we still don't have if- and while-let-chains
    // https://github.com/rust-lang/rust/issues/53667
    if let Some(orig_v) = original.get(key).map(|x| x.as_str()) {
      if orig_v != value {
        modified.insert(key.to_owned(), value.to_owned());
      }
      removed.remove(key);
    } else {
      added.insert(key.to_owned(), value.to_owned());
    }
  }
  EnvDiff {
    added,
    removed: removed.into_iter().map(|x| x.to_owned()).collect(),
    modified,
  }
}

#[derive(Debug, Clone)]
pub struct BaselineInfo {
  pub cwd: PathBuf,
  pub env: BTreeMap<String, String>,
  pub fdinfo: FileDescriptorInfoCollection,
}

impl BaselineInfo {
  pub fn new() -> color_eyre::Result<Self> {
    let cwd = std::env::current_dir()?;
    let env = std::env::vars().collect();
    let fdinfo = FileDescriptorInfoCollection::new_baseline()?;
    Ok(Self { cwd, env, fdinfo })
  }

  pub fn with_pts(pts: &UnixSlavePty) -> color_eyre::Result<Self> {
    let cwd = std::env::current_dir()?;
    let env = std::env::vars().collect();
    let fdinfo = FileDescriptorInfoCollection::with_pts(pts)?;
    Ok(Self { cwd, env, fdinfo })
  }
}
