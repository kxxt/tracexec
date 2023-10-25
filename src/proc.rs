use core::fmt;
use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    fmt::{Display, Formatter},
    io::{self, BufRead, BufReader, Read},
    path::PathBuf,
};

use color_eyre::owo_colors::OwoColorize;
use log::debug;
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
    NonUTF8,
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
            Interpreter::NonUTF8 => write!(f, "({}: {})", "err".red().bold(), "Not valid UTF-8"),
        }
    }
}

pub fn read_interpreter_recursive(exe: &str) -> Vec<Interpreter> {
    let mut exe = Cow::Borrowed(exe);
    let mut interpreters = Vec::new();
    loop {
        match read_interpreter(&exe) {
            Interpreter::Shebang(path) => {
                // TODO: maybe we can remove this clone
                exe = Cow::Owned(path.clone());
                interpreters.push(Interpreter::Shebang(path));
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
    debug!("File: {exe:?}, Shebang: {:?}", buf);
    if &buf != b"#!" {
        return Interpreter::None;
    }
    // Read the rest of the line
    let mut buf = Vec::new();

    if let Err(e) = reader.read_until(b'\n', &mut buf) {
        return Interpreter::Error(e);
    };
    // Get the interpreter
    let Ok(buf) = String::from_utf8(buf) else {
        return Interpreter::NonUTF8;
    };
    debug!("Shebang: {:?}", buf);
    buf.split_ascii_whitespace()
        .next()
        .map(|x| x.to_string())
        .map(Interpreter::Shebang)
        .unwrap_or(Interpreter::Shebang(String::new()))
}
