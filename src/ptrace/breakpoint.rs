use std::{borrow::Cow, error::Error};

use nix::unistd::Pid;
use regex_cursor::engines::pikevm::{self, PikeVM};
use strum::IntoStaticStr;

use crate::{
  event::OutputMsg,
  regex::{ArgvCursor, SPACE},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakPointHit {
  pub bid: u32,
  pub pid: Pid,
  pub stop: BreakPointStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoStaticStr)]
pub enum BreakPointStop {
  SyscallEnter,
  SyscallExit,
}

impl BreakPointStop {
  pub fn toggle(&mut self) {
    *self = match self {
      Self::SyscallEnter => Self::SyscallExit,
      Self::SyscallExit => Self::SyscallEnter,
    }
  }
}

#[derive(Debug, Clone)]
pub struct BreakPointRegex {
  regex: PikeVM,
  editable: String,
}

#[derive(Debug, Clone)]
pub enum BreakPointPattern {
  /// A regular expression that matches the cmdline of the process. The cmdline is the argv
  /// concatenated with spaces without any escaping.
  ArgvRegex(BreakPointRegex),
  // CmdlineRegex(BreakPointRegex),
  InFilename(String),
  ExactFilename(String),
}

#[derive(Debug, Clone)]
pub enum BreakPointType {
  /// The breakpoint will be hit once and then deactivated.
  Once,
  /// The breakpoint will be hit every time it is encountered.
  Permanent,
}

#[derive(Debug, Clone)]
pub struct BreakPoint {
  pub pattern: BreakPointPattern,
  pub ty: BreakPointType,
  pub activated: bool,
  pub stop: BreakPointStop,
}

impl BreakPointPattern {
  pub fn pattern(&self) -> &str {
    match self {
      Self::ArgvRegex(regex) => regex.editable.as_str(),
      Self::InFilename(filename) => filename,
      // Unwrap is fine since user inputs the filename as str
      Self::ExactFilename(filename) => filename,
    }
  }

  pub fn to_editable(&self) -> String {
    match self {
      Self::ArgvRegex(regex) => format!("argv-regex:{}", regex.editable),
      Self::InFilename(filename) => format!("in-filename:{}", filename),
      Self::ExactFilename(filename) => {
        format!("exact-filename:{}", filename)
      }
    }
  }

  pub fn from_editable(editable: &str) -> Result<Self, String> {
    if let Some((prefix, rest)) = editable.split_once(':') {
      match prefix {
        "in-filename" => Ok(Self::InFilename(rest.to_string())),
        "exact-filename" => Ok(Self::ExactFilename(rest.to_string())),
        "argv-regex" => Ok(Self::ArgvRegex(BreakPointRegex {
          regex: PikeVM::new(rest).map_err(|e| e.to_string())?,
          editable: rest.to_string(),
        })),
        _ => Err(format!("Invalid breakpoint pattern type: {prefix}!")),
      }
    } else {
      Err("No valid breakpoint pattern found!".to_string())
    }
  }

  pub fn matches(&self, argv: Option<&[OutputMsg]>, filename: &OutputMsg) -> bool {
    match self {
      Self::ArgvRegex(regex) => {
        let Some(argv) = argv else {
          return false;
        };
        let space = &SPACE;
        let argv = ArgvCursor::new(argv, space);
        pikevm::is_match(
          &regex.regex,
          &mut pikevm::Cache::new(&regex.regex),
          &mut regex_cursor::Input::new(argv),
        )
      }
      Self::InFilename(pattern) => {
        let OutputMsg::Ok(filename) = filename else {
          return false;
        };
        filename.contains(pattern)
      }
      Self::ExactFilename(path) => {
        let OutputMsg::Ok(filename) = filename else {
          return false;
        };
        filename == path
      }
    }
  }
}

impl TryFrom<&str> for BreakPoint {
  type Error = Cow<'static, str>;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    let Some((stop, rest)) = value.split_once(':') else {
      return Err("No valid syscall stop found! The breakpoint should start with \"sysenter:\" or \"sysexit:\".".into());
    };
    let stop = match stop {
      "sysenter" => BreakPointStop::SyscallEnter,
      "sysexit" => BreakPointStop::SyscallExit,
      _ => {
        return Err(
          format!("Invalid syscall stop {stop:?}! The breakpoint should start with \"sysenter:\" or \"sysexit:\".")
            .into(),
        )
      }
    };
    let Some((pattern_kind, pattern)) = rest.split_once(':') else {
      return Err("No valid pattern kind found! The breakpoint pattern should start with \"argv-regex:\", \"exact-filename:\" or \"in-filename:\".".into());
    };
    let pattern = match pattern_kind {
      "argv-regex" => BreakPointPattern::ArgvRegex(BreakPointRegex {
        regex: PikeVM::new(pattern).map_err(|e| format!("\n{}", e.source().unwrap()))?,
        editable: pattern.to_string(),
      }),
      "exact-filename" => BreakPointPattern::ExactFilename(pattern.to_string()),
      "in-filename" => BreakPointPattern::InFilename(pattern.to_string()),
      _ => {
        return Err(
          format!(
            "Invalid pattern kind {pattern_kind:?}! The breakpoint pattern should start with \"argv-regex:\", \"exact-filename:\" or \"in-filename:\"."
          )
          .into(),
        )
      }
    };
    Ok(Self {
      ty: BreakPointType::Permanent,
      stop,
      pattern,
      activated: true,
    })
  }
}
