use std::{
  borrow::Cow,
  error::Error,
};

use nix::unistd::Pid;
use regex_cursor::engines::pikevm::{
  self,
  PikeVM,
};
use strum::IntoStaticStr;

use crate::{
  event::OutputMsg,
  primitives::regex::{
    ArgvCursor,
    SPACE,
  },
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
      Self::InFilename(filename) => format!("in-filename:{filename}"),
      Self::ExactFilename(filename) => {
        format!("exact-filename:{filename}")
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
        filename.as_str() == path
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

#[cfg(test)]
mod tests {
  use nix::errno::Errno;

  use super::*;
  use crate::cache::ArcStr;

  #[test]
  fn test_breakpoint_stop_toggle() {
    let mut s = BreakPointStop::SyscallEnter;
    s.toggle();
    assert_eq!(s, BreakPointStop::SyscallExit);
    s.toggle();
    assert_eq!(s, BreakPointStop::SyscallEnter);
  }

  #[test]
  fn test_from_editable_and_to_editable_and_pattern() {
    // argv-regex
    let bp = BreakPointPattern::from_editable("argv-regex:foo").expect("argv-regex");
    assert_eq!(bp.pattern(), "foo");
    assert_eq!(bp.to_editable(), "argv-regex:foo");

    // in-filename
    let bp2 = BreakPointPattern::from_editable("in-filename:/tmp/test").expect("in-filename");
    assert_eq!(bp2.pattern(), "/tmp/test");
    assert_eq!(bp2.to_editable(), "in-filename:/tmp/test");

    // exact-filename
    let bp3 = BreakPointPattern::from_editable("exact-filename:/bin/sh").expect("exact-filename");
    assert_eq!(bp3.pattern(), "/bin/sh");
    assert_eq!(bp3.to_editable(), "exact-filename:/bin/sh");

    // invalid prefix
    assert!(BreakPointPattern::from_editable("unknown:abc").is_err());
    // missing colon
    assert!(BreakPointPattern::from_editable("no-colon").is_err());
  }

  #[test]
  fn test_matches_argv_regex() {
    // pattern "arg1" should match when argv contains "arg1"
    let pat = BreakPointPattern::from_editable("argv-regex:arg1").unwrap();

    let argv = [
      OutputMsg::Ok(ArcStr::from("arg0")),
      OutputMsg::Ok(ArcStr::from("arg1")),
      OutputMsg::Ok(ArcStr::from("arg2")),
    ];
    let filename = OutputMsg::Ok(ArcStr::from("/bin/prog"));

    assert!(pat.matches(Some(&argv), &filename));

    // If argv is None, ArgvRegex cannot match
    assert!(!pat.matches(None, &filename));
  }

  #[test]
  fn test_matches_in_and_exact_filename() {
    let in_pat = BreakPointPattern::from_editable("in-filename:log").unwrap();
    let exact_pat = BreakPointPattern::from_editable("exact-filename:/var/log/app").unwrap();

    let ok_filename = OutputMsg::Ok(ArcStr::from("/var/log/app"));
    let other_filename = OutputMsg::Ok(ArcStr::from("/tmp/file"));
    let partial_filename = OutputMsg::PartialOk(ArcStr::from("something"));
    let err_filename = OutputMsg::Err(crate::event::FriendlyError::InspectError(Errno::EINVAL));

    // in-filename: substring match only when filename is Ok
    assert!(in_pat.matches(Some(&[]), &ok_filename));
    assert!(!in_pat.matches(Some(&[]), &other_filename));
    assert!(!in_pat.matches(Some(&[]), &partial_filename));
    assert!(!in_pat.matches(Some(&[]), &err_filename));

    // exact-filename: equality only when filename is Ok
    assert!(exact_pat.matches(Some(&[]), &ok_filename));
    assert!(!exact_pat.matches(Some(&[]), &other_filename));
    assert!(!exact_pat.matches(Some(&[]), &partial_filename));
    assert!(!exact_pat.matches(Some(&[]), &err_filename));
  }

  #[test]
  fn test_try_from_breakpoint_valid_and_invalid() {
    // valid sysenter argv regex
    let bp = BreakPoint::try_from("sysenter:argv-regex:foo").expect("valid breakpoint");
    assert_eq!(bp.stop, BreakPointStop::SyscallEnter);
    match bp.pattern {
      BreakPointPattern::ArgvRegex(r) => assert_eq!(r.editable, "foo"),
      _ => panic!("expected ArgvRegex"),
    }

    // valid sysexit exact filename
    let bp2 =
      BreakPoint::try_from("sysexit:exact-filename:/bin/ls").expect("valid exact breakpoint");
    assert_eq!(bp2.stop, BreakPointStop::SyscallExit);
    match bp2.pattern {
      BreakPointPattern::ExactFilename(s) => assert_eq!(s, "/bin/ls"),
      _ => panic!("expected ExactFilename"),
    }

    // missing stop
    assert!(BreakPoint::try_from("no-colon-here").is_err());

    // invalid stop
    assert!(BreakPoint::try_from("badstop:argv-regex:foo").is_err());

    // missing pattern kind
    assert!(BreakPoint::try_from("sysenter:badformat").is_err());

    // invalid pattern kind
    assert!(BreakPoint::try_from("sysenter:unknown-kind:xyz").is_err());
  }
}
