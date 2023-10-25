use core::fmt;
use std::{
    borrow::Cow,
    ffi::CString,
    fmt::{Display, Formatter},
    io::{self, BufRead, BufReader, Read},
    path::PathBuf,
};

use color_eyre::owo_colors::OwoColorize;

use nix::unistd::Pid;

pub fn read_argv(pid: Pid) -> color_eyre::Result<Vec<CString>> {
    let filename = format!("/proc/{pid}/cmdline");
    let buf = std::fs::read(filename)?;
    Ok(buf
        .split(|&c| c == 0)
        .map(CString::new)
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn read_comm(pid: Pid) -> color_eyre::Result<String> {
    let filename = format!("/proc/{pid}/comm");
    let mut buf = std::fs::read(filename)?;
    buf.pop(); // remove trailing newline
    Ok(String::from_utf8(buf)?)
}

pub fn read_cwd(pid: Pid) -> color_eyre::Result<PathBuf> {
    let filename = format!("/proc/{pid}/cwd");
    let buf = std::fs::read_link(filename)?;
    Ok(buf)
}

#[derive(Debug)]
pub enum Interpreter {
    None,
    Shebang(String),
    ExecutableUnaccessible,
    Error(io::Error),
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

pub fn read_interpreter_recursive(exe: &str) -> Vec<Interpreter> {
    let mut exe = Cow::Borrowed(exe);
    let mut interpreters = Vec::new();
    loop {
        match read_interpreter(&exe) {
            Interpreter::Shebang(shebang) => {
                exe = Cow::Owned(
                    shebang
                        .split_ascii_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_owned(),
                );
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

pub fn read_interpreter(exe: &str) -> Interpreter {
    fn err_to_interpreter(e: io::Error) -> Interpreter {
        if e.kind() == io::ErrorKind::PermissionDenied || e.kind() == io::ErrorKind::NotFound {
            Interpreter::ExecutableUnaccessible
        } else {
            Interpreter::Error(e)
        }
    }
    let file = match std::fs::File::open(&exe) {
        Ok(file) => file,
        Err(e) => return err_to_interpreter(e),
    };
    let mut reader = BufReader::new(file);
    // First, check if it's a shebang script
    let mut buf = [0u8; 2];

    if let Err(e) = reader.read_exact(&mut buf) {
        return Interpreter::Error(e);
    };
    if &buf != b"#!" {
        return Interpreter::None;
    }
    // Read the rest of the line
    let mut buf = Vec::new();

    if let Err(e) = reader.read_until(b'\n', &mut buf) {
        return Interpreter::Error(e);
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
