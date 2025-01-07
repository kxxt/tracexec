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
  sync::{Arc, LazyLock, RwLock},
};

use crate::cache::ArcStr;
use filedescriptor::AsRawFileDescriptor;
use owo_colors::OwoColorize;

use nix::{
  fcntl::OFlag,
  libc::AT_FDCWD,
  unistd::{getpid, Pid},
};
use serde::{ser::SerializeSeq, Serialize, Serializer};
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
  let mut cache = CACHE.write().unwrap();
  Ok(cache.get_or_insert(&utf8))
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
      path: OutputMsg::Ok("".into()),
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
        let mut cache = CACHE.write().unwrap();
        let line = cache.get_or_insert_owned(line);
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
      let mut cache = CACHE.write().unwrap();
      return Ok(cache.get_or_insert_owned(line));
    }
  }
  let mut cache = CACHE.write().unwrap();
  Ok(cache.get_or_insert("Not found. This is probably a pipe or something else."))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "what", content = "value", rename_all = "kebab-case")]
pub enum Interpreter {
  None,
  Shebang(ArcStr),
  ExecutableUnaccessible,
  Error(ArcStr),
}

impl Display for Interpreter {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::None => write!(f, "{}", "none".bold()),
      Self::Shebang(s) => write!(f, "{:?}", s),
      Self::ExecutableUnaccessible => {
        write!(f, "{}", "executable unaccessible".red().bold())
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
      Interpreter::ExecutableUnaccessible
    } else {
      let mut cache = CACHE.write().unwrap();
      let e = cache.get_or_insert_owned(e.to_string());
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
    let mut cache = CACHE.write().unwrap();
    let e = cache.get_or_insert_owned(e.to_string());
    return Interpreter::Error(e);
  };
  if &buf != b"#!" {
    return Interpreter::None;
  }
  // Read the rest of the line
  let mut buf = Vec::new();

  if let Err(e) = reader.read_until(b'\n', &mut buf) {
    let mut cache = CACHE.write().unwrap();
    let e = cache.get_or_insert_owned(e.to_string());
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
  let mut cache = CACHE.write().unwrap();
  let shebang = cache.get_or_insert(&shebang);
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
  (head, &tail[1..])
}

pub fn parse_envp(envp: Vec<String>) -> BTreeMap<OutputMsg, OutputMsg> {
  envp
    .into_iter()
    .map(|entry| {
      let (key, value) = parse_env_entry(&entry);
      let mut cache = CACHE.write().unwrap();
      (
        OutputMsg::Ok(cache.get_or_insert(key)),
        OutputMsg::Ok(cache.get_or_insert(value)),
      )
    })
    .collect()
}

pub fn parse_failiable_envp(envp: Vec<OutputMsg>) -> BTreeMap<OutputMsg, OutputMsg> {
  envp
    .into_iter()
    .map(|entry| {
      if let OutputMsg::Ok(s) | OutputMsg::PartialOk(s) = entry {
        let (key, value) = parse_env_entry(&s);
        let mut cache = CACHE.write().unwrap();
        (
          OutputMsg::Ok(cache.get_or_insert(key)),
          OutputMsg::Ok(cache.get_or_insert(value)),
        )
      } else {
        (entry.clone(), entry)
      }
    })
    .collect()
}

pub fn cached_str(s: &str) -> ArcStr {
  let mut cache = CACHE.write().unwrap();
  cache.get_or_insert(s)
}

pub fn cached_string(s: String) -> ArcStr {
  let mut cache = CACHE.write().unwrap();
  cache.get_or_insert_owned(s)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvDiff {
  pub added: BTreeMap<OutputMsg, OutputMsg>,
  pub removed: BTreeSet<OutputMsg>,
  pub modified: BTreeMap<OutputMsg, OutputMsg>,
}

impl EnvDiff {
  pub fn is_modified_or_removed(&self, key: &OutputMsg) -> bool {
    self.modified.contains_key(key) || self.removed.contains(key)
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
  for (key, value) in envp.iter() {
    // Too bad that we still don't have if- and while-let-chains
    // https://github.com/rust-lang/rust/issues/53667
    if let Some(orig_v) = original.get(key) {
      if orig_v != value {
        modified.insert(key.clone(), value.clone());
      }
      removed.remove(key);
    } else {
      added.insert(key.clone(), value.clone());
    }
  }
  EnvDiff {
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
        let mut cache = CACHE.write().unwrap();
        (
          cache.get_or_insert_owned(k).into(),
          cache.get_or_insert_owned(v).into(),
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
        let mut cache = CACHE.write().unwrap();
        (
          cache.get_or_insert_owned(k).into(),
          cache.get_or_insert_owned(v).into(),
        )
      })
      .collect();
    let fdinfo = FileDescriptorInfoCollection::with_pts(pts)?;
    Ok(Self { cwd, env, fdinfo })
  }
}

static CACHE: LazyLock<Arc<RwLock<StringCache>>> =
  LazyLock::new(|| Arc::new(RwLock::new(StringCache::new())));
