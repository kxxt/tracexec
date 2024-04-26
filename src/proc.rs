use core::fmt;
use std::{
  borrow::Cow,
  ffi::CString,
  fmt::{Display, Formatter},
  io::{self, BufRead, BufReader, Read},
  path::{Path, PathBuf},
};

use color_eyre::owo_colors::OwoColorize;

use nix::{libc::AT_FDCWD, unistd::Pid};

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

pub fn read_fd(pid: Pid, fd: i32) -> std::io::Result<PathBuf> {
  if fd == AT_FDCWD {
    return read_cwd(pid);
  }
  let filename = format!("/proc/{pid}/fd/{fd}");
  std::fs::read_link(filename)
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
  // Get trimed shebang line [start, end) indices
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
  let mut sep_loc = item
    .as_bytes()
    .iter()
    .position(|&x| x == b'=')
    .unwrap_or_else(|| {
      log::warn!(
        "Invalid envp entry: {:?}, assuming value to empty string!",
        item
      );
      item.len()
    });
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
