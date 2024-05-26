use std::{
  borrow::Cow,
  collections::{BTreeMap, HashMap},
  error::Error,
  path::{Path, PathBuf},
  sync::Arc,
};

use arcstr::ArcStr;
use nix::{sys::signal::Signal, unistd::Pid};
use regex_cursor::engines::pikevm::{self, PikeVM};
use strum::IntoStaticStr;

use crate::{
  proc::{read_comm, FileDescriptorInfoCollection, Interpreter},
  regex::{ArgvCursor, SPACE},
  tracer::InspectError,
};

pub struct ProcessStateStore {
  processes: HashMap<Pid, Vec<ProcessState>>,
}

#[derive(Debug)]
pub struct PendingDetach {
  pub hid: u64,
  pub signal: Signal,
}

#[derive(Debug)]
pub struct ProcessState {
  pub pid: Pid,
  pub ppid: Option<Pid>,
  pub status: ProcessStatus,
  pub start_time: u64,
  pub comm: ArcStr,
  pub presyscall: bool,
  pub is_exec_successful: bool,
  pub syscall: i64,
  pub exec_data: Option<ExecData>,
  pub associated_events: Vec<u64>,
  /// A pending detach request with a signal to send to the process
  pub pending_detach: Option<PendingDetach>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessExit {
  Code(i32),
  Signal(Signal),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
  Initialized,
  SigstopReceived,
  PtraceForkEventReceived,
  Running,
  Exited(ProcessExit),
  BreakPointHit,
  Detached,
}

#[derive(Debug)]
pub struct ExecData {
  pub filename: Result<PathBuf, InspectError>,
  pub argv: Arc<Result<Vec<ArcStr>, InspectError>>,
  pub envp: Arc<Result<BTreeMap<ArcStr, ArcStr>, InspectError>>,
  pub cwd: PathBuf,
  pub interpreters: Vec<Interpreter>,
  pub fdinfo: Arc<FileDescriptorInfoCollection>,
}

impl ExecData {
  pub fn new(
    filename: Result<PathBuf, InspectError>,
    argv: Result<Vec<ArcStr>, InspectError>,
    envp: Result<BTreeMap<ArcStr, ArcStr>, InspectError>,
    cwd: PathBuf,
    interpreters: Vec<Interpreter>,
    fdinfo: FileDescriptorInfoCollection,
  ) -> Self {
    Self {
      filename,
      argv: Arc::new(argv),
      envp: Arc::new(envp),
      cwd,
      interpreters,
      fdinfo: Arc::new(fdinfo),
    }
  }
}

impl ProcessStateStore {
  #[allow(clippy::new_without_default)]
  pub fn new() -> Self {
    Self {
      processes: HashMap::new(),
    }
  }

  pub fn insert(&mut self, state: ProcessState) {
    self.processes.entry(state.pid).or_default().push(state);
  }

  pub fn get_current_mut(&mut self, pid: Pid) -> Option<&mut ProcessState> {
    // The last process in the vector is the current process
    // println!("Getting {pid}");
    self.processes.get_mut(&pid)?.last_mut()
  }

  pub fn get_current(&self, pid: Pid) -> Option<&ProcessState> {
    // The last process in the vector is the current process
    self.processes.get(&pid)?.last()
  }
}

impl ProcessState {
  pub fn new(pid: Pid, start_time: u64) -> color_eyre::Result<Self> {
    Ok(Self {
      pid,
      ppid: None,
      status: ProcessStatus::Initialized,
      comm: read_comm(pid)?,
      start_time,
      presyscall: true,
      is_exec_successful: false,
      syscall: -1,
      exec_data: None,
      associated_events: Vec::new(),
      pending_detach: None,
    })
  }

  pub fn associate_event(&mut self, id: impl IntoIterator<Item = u64>) {
    self.associated_events.extend(id);
  }
}

#[derive(Debug, Clone, Copy, PartialEq, IntoStaticStr)]
pub enum BreakPointStop {
  SyscallEnter,
  SyscallExit,
}

impl BreakPointStop {
  pub fn toggle(&mut self) {
    *self = match self {
      BreakPointStop::SyscallEnter => BreakPointStop::SyscallExit,
      BreakPointStop::SyscallExit => BreakPointStop::SyscallEnter,
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
  ExactFilename(PathBuf),
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
      BreakPointPattern::ArgvRegex(regex) => regex.editable.as_str(),
      BreakPointPattern::InFilename(filename) => filename,
      // Unwrap is fine since user inputs the filename as str
      BreakPointPattern::ExactFilename(filename) => filename.to_str().unwrap(),
    }
  }

  pub fn to_editable(&self) -> String {
    match self {
      BreakPointPattern::ArgvRegex(regex) => format!("argv-regex:{}", regex.editable),
      BreakPointPattern::InFilename(filename) => format!("in-filename:{}", filename),
      BreakPointPattern::ExactFilename(filename) => {
        format!("exact-filename:{}", filename.to_str().unwrap())
      }
    }
  }

  pub fn from_editable(editable: &str) -> Result<Self, String> {
    if let Some((prefix, rest)) = editable.split_once(':') {
      match prefix {
        "in-filename" => Ok(Self::InFilename(rest.to_string())),
        "exact-filename" => Ok(Self::ExactFilename(PathBuf::from(rest))),
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

  pub fn matches(&self, argv: Option<&[ArcStr]>, filename: Option<&Path>) -> bool {
    match self {
      BreakPointPattern::ArgvRegex(regex) => {
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
      BreakPointPattern::InFilename(pattern) => {
        let Some(filename) = filename else {
          return false;
        };
        filename.to_string_lossy().contains(pattern)
      }
      BreakPointPattern::ExactFilename(path) => {
        let Some(filename) = filename else {
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
        regex: PikeVM::new(pattern).map_err(|e| format!("\n{}", e.source().unwrap().to_string()))?,
        editable: pattern.to_string(),
      }),
      "exact-filename" => BreakPointPattern::ExactFilename(PathBuf::from(pattern)),
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
