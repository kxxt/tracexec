use std::{
  borrow::Cow,
  ops::Range,
};

use itertools::Itertools;
use nix::fcntl::OFlag;

use crate::{
  cli::args::ModifierArgs,
  event::{
    ExecEvent,
    OutputMsg,
    RuntimeModifier,
    TracerEventDetails,
  },
  proc::{
    BaselineInfo,
    FileDescriptorInfoCollection,
  },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyTarget {
  Line,
  Commandline(SupportedShell),
  CommandlineWithFullEnv(SupportedShell),
  CommandlineWithStdio(SupportedShell),
  CommandlineWithFds(SupportedShell),
  Env,
  Argv,
  ArgvJoined,
  Filename,
  SyscallResult,
  EnvDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedShell {
  Bash,
  Sh,
  Fish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandlinePartKind {
  Plain,
  TracerEvent,
  Arg0,
  Cwd,
  DeletedEnvVar,
  AddedEnvVar,
  ModifiedEnvVar,
  UnchangedEnvKey,
  UnchangedEnvVal,
  Filename,
  Argv,
  InlineTracerError,
  ModifiedFdInCommandline,
  RemovedFdInCommandline,
  CloexecFdInCommandline,
  AddedFdInCommandline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStatus {
  Ok,
  PartialOk,
  Err,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandlinePart {
  pub text: String,
  pub kind: CommandlinePartKind,
  pub output_status: Option<OutputStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commandline {
  pub parts: Vec<CommandlinePart>,
  pub cwd_range: Option<Range<usize>>,
  pub env_range: Option<Range<usize>>,
}

impl std::fmt::Display for Commandline {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for part in &self.parts {
      f.write_str(&part.text)?;
    }
    Ok(())
  }
}

impl Commandline {
  fn push(&mut self, text: impl Into<String>, kind: CommandlinePartKind) {
    self.parts.push(CommandlinePart {
      text: text.into(),
      kind,
      output_status: None,
    });
  }

  fn push_bash_escaped(&mut self, msg: &OutputMsg, kind: CommandlinePartKind) {
    self.parts.push(CommandlinePart {
      text: msg.bash_escaped().into_owned(),
      kind,
      output_status: Some(output_status(msg)),
    });
  }
}

fn output_status(msg: &OutputMsg) -> OutputStatus {
  match msg {
    OutputMsg::Ok(_) => OutputStatus::Ok,
    OutputMsg::PartialOk(_) => OutputStatus::PartialOk,
    OutputMsg::Err(_) => OutputStatus::Err,
  }
}

fn handle_stdio_fd(
  fd: i32,
  fdinfo_orig: &crate::proc::FileDescriptorInfo,
  curr: &FileDescriptorInfoCollection,
  commandline: &mut Commandline,
) {
  let (fdstr, redir) = match fd {
    0 => (" 0", "<"),
    1 => (" 1", ">"),
    2 => (" 2", "2>"),
    _ => unreachable!(),
  };

  if let Some(fdinfo) = curr.get(fd) {
    if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
      commandline.push(fdstr, CommandlinePartKind::CloexecFdInCommandline);
      commandline.push(">&-", CommandlinePartKind::CloexecFdInCommandline);
    } else if fdinfo.not_same_file_as(fdinfo_orig) {
      commandline.push(" ", CommandlinePartKind::Plain);
      commandline.push(redir, CommandlinePartKind::ModifiedFdInCommandline);
      commandline.push_bash_escaped(&fdinfo.path, CommandlinePartKind::ModifiedFdInCommandline);
    }
  } else if curr.is_reliable() {
    commandline.push(fdstr, CommandlinePartKind::CloexecFdInCommandline);
    commandline.push(">&-", CommandlinePartKind::RemovedFdInCommandline);
  }
}

pub fn exec_commandline(
  event: &ExecEvent,
  baseline: &BaselineInfo,
  modifier: &ModifierArgs,
  rt_modifier: RuntimeModifier,
  full_env: bool,
  _shell: SupportedShell,
) -> Commandline {
  let mut commandline = Commandline {
    parts: Vec::new(),
    cwd_range: None,
    env_range: None,
  };
  commandline.push("env", CommandlinePartKind::TracerEvent);

  let ExecEvent {
    cwd,
    filename,
    argv,
    env_diff,
    fdinfo,
    envp,
    has_dash_env,
    ..
  } = event;

  // Handle argv[0]
  let _ = argv.as_deref().inspect(|v| {
    v.first().inspect(|&arg0| {
      if filename != arg0 {
        commandline.push(" ", CommandlinePartKind::Plain);
        commandline.push("-a ", CommandlinePartKind::Arg0);
        commandline.push_bash_escaped(arg0, CommandlinePartKind::Arg0);
      }
    });
  });

  // Handle cwd
  if cwd != &baseline.cwd && rt_modifier.show_cwd {
    let start = commandline.parts.len();
    commandline.push(" ", CommandlinePartKind::Plain);
    commandline.push("-C ", CommandlinePartKind::Cwd);
    commandline.push_bash_escaped(cwd, CommandlinePartKind::Cwd);
    commandline.cwd_range = Some(start..commandline.parts.len());
  }

  // Handle env
  if rt_modifier.show_env {
    let start = commandline.parts.len();
    if !full_env {
      if let Ok(env_diff) = env_diff {
        for k in env_diff.removed.iter() {
          commandline.push(" ", CommandlinePartKind::Plain);
          commandline.push("-u ", CommandlinePartKind::DeletedEnvVar);
          commandline.push_bash_escaped(k, CommandlinePartKind::DeletedEnvVar);
        }
        if env_diff.need_env_argument_separator(filename) {
          commandline.push(" ", CommandlinePartKind::Plain);
          commandline.push("--", CommandlinePartKind::Plain);
        }
        for (k, v) in env_diff.added.iter() {
          commandline.push(" ", CommandlinePartKind::Plain);
          commandline.push_bash_escaped(k, CommandlinePartKind::AddedEnvVar);
          commandline.push("=", CommandlinePartKind::AddedEnvVar);
          commandline.push_bash_escaped(v, CommandlinePartKind::AddedEnvVar);
        }
        for (k, v) in env_diff.modified.iter() {
          commandline.push(" ", CommandlinePartKind::Plain);
          commandline.push_bash_escaped(k, CommandlinePartKind::ModifiedEnvVar);
          commandline.push("=", CommandlinePartKind::ModifiedEnvVar);
          commandline.push_bash_escaped(v, CommandlinePartKind::ModifiedEnvVar);
        }
      }
    } else if let Ok(envp) = &**envp {
      commandline.push(" ", CommandlinePartKind::Plain);
      commandline.push("-i", CommandlinePartKind::Plain);
      if *has_dash_env || (envp.is_empty() && filename.as_ref().starts_with('-')) {
        commandline.push(" --", CommandlinePartKind::Plain);
      }
      for (k, v) in envp.iter() {
        commandline.push(" ", CommandlinePartKind::Plain);
        commandline.push_bash_escaped(k, CommandlinePartKind::UnchangedEnvKey);
        commandline.push("=", CommandlinePartKind::UnchangedEnvKey);
        commandline.push_bash_escaped(v, CommandlinePartKind::UnchangedEnvVal);
      }
    }
    commandline.env_range = Some(start..commandline.parts.len());
  }

  commandline.push(" ", CommandlinePartKind::Plain);
  // Filename
  commandline.push_bash_escaped(filename, CommandlinePartKind::Filename);

  // Argv[1..]
  match argv.as_ref() {
    Ok(argv) => {
      for arg in argv.iter().skip(1) {
        commandline.push(" ", CommandlinePartKind::Plain);
        commandline.push_bash_escaped(arg, CommandlinePartKind::Argv);
      }
    }
    Err(_) => {
      commandline.push(" ", CommandlinePartKind::Plain);
      commandline.push(
        "[failed to read argv]",
        CommandlinePartKind::InlineTracerError,
      );
    }
  }

  // FD
  if modifier.stdio_in_cmdline {
    for (fd, fdinfo_orig) in baseline.fdinfo.stdio() {
      handle_stdio_fd(fd, fdinfo_orig, fdinfo, &mut commandline);
    }
  }

  if modifier.fd_in_cmdline {
    for (&fd, fdinfo) in fdinfo.fdinfo.iter() {
      if fd < 3 {
        continue;
      }
      if fdinfo.flags.ok().is_none() || fdinfo.flags.intersects(OFlag::O_CLOEXEC) {
        // Skip fds that will be closed upon exec
        continue;
      }
      commandline.push(" ", CommandlinePartKind::Plain);
      commandline.push(fd.to_string(), CommandlinePartKind::AddedFdInCommandline);
      commandline.push("<>", CommandlinePartKind::AddedFdInCommandline);
      commandline.push_bash_escaped(&fdinfo.path, CommandlinePartKind::AddedFdInCommandline);
    }
  }

  commandline
}

pub fn text_for_copy<'a>(
  event_details: &'a TracerEventDetails,
  baseline: &BaselineInfo,
  target: CopyTarget,
  _modifier_args: &ModifierArgs,
  _rt_modifier: RuntimeModifier,
) -> Cow<'a, str> {
  let TracerEventDetails::Exec(event) = event_details else {
    panic!("Copy target {target:?} is only available for Exec events");
  };
  let mut modifier_args = ModifierArgs::default();
  match target {
    CopyTarget::Commandline(shell) => exec_commandline(
      event,
      baseline,
      &modifier_args,
      RuntimeModifier::default(),
      false,
      shell,
    )
    .to_string()
    .into(),
    CopyTarget::CommandlineWithFullEnv(shell) => exec_commandline(
      event,
      baseline,
      &modifier_args,
      RuntimeModifier::default(),
      true,
      shell,
    )
    .to_string()
    .into(),
    CopyTarget::CommandlineWithStdio(shell) => {
      modifier_args.stdio_in_cmdline = true;
      exec_commandline(
        event,
        baseline,
        &modifier_args,
        RuntimeModifier::default(),
        false,
        shell,
      )
      .to_string()
      .into()
    }
    CopyTarget::CommandlineWithFds(shell) => {
      modifier_args.fd_in_cmdline = true;
      modifier_args.stdio_in_cmdline = true;
      exec_commandline(
        event,
        baseline,
        &modifier_args,
        RuntimeModifier::default(),
        false,
        shell,
      )
      .to_string()
      .into()
    }
    CopyTarget::Env => match event.envp.as_ref() {
      Ok(envp) => envp
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .join("\n")
        .into(),
      Err(e) => format!("[failed to read envp: {e}]").into(),
    },
    CopyTarget::EnvDiff => {
      let Ok(env_diff) = event.env_diff.as_ref() else {
        return "[failed to read envp]".into();
      };
      let mut result = String::new();
      result.push_str("# Added:\n");
      for (k, v) in env_diff.added.iter() {
        result.push_str(&format!("{k}={v}\n"));
      }
      result.push_str("# Modified: (original first)\n");
      for (k, original, current) in env_diff.modified_with_values(&baseline.env) {
        result.push_str(&format!("{}={}\n{}={}\n", k, original, k, current));
      }
      result.push_str("# Removed:\n");
      for (k, value) in env_diff.removed_with_values(&baseline.env) {
        result.push_str(&format!("{k}={value}\n"));
      }
      result.into()
    }
    CopyTarget::Argv => TracerEventDetails::argv_to_string(&event.argv).into(),
    CopyTarget::ArgvJoined => match event.argv.as_ref() {
      Ok(argv) => argv.iter().map(AsRef::as_ref).join(" ").into(),
      Err(_) => "[failed to read argv]".into(),
    },
    CopyTarget::Filename => Cow::Borrowed(event.filename.as_ref()),
    CopyTarget::SyscallResult => event.result.to_string().into(),
    CopyTarget::Line => panic!("CopyTarget::Line requires a presentation formatter"),
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    sync::Arc,
  };

  use nix::unistd::Pid;

  use super::*;
  use crate::{
    event::ExecSyscall,
    proc::{
      CgroupInfo,
      Cred,
      diff_env,
    },
  };

  fn msg(value: &str) -> OutputMsg {
    OutputMsg::Ok(value.into())
  }

  fn filename_event(
    baseline: &BaselineInfo,
    envp: BTreeMap<OutputMsg, OutputMsg>,
    filename: &str,
  ) -> ExecEvent {
    let has_dash_env = envp.keys().any(|key| key.as_ref().starts_with('-'));
    let filename = msg(filename);
    ExecEvent {
      syscall: ExecSyscall::Execve,
      exec_pid: Pid::from_raw(1),
      pid: Pid::from_raw(1),
      cwd: baseline.cwd.clone(),
      comm: "false".into(),
      filename: filename.clone(),
      argv: Arc::new(Ok(vec![filename])),
      env_diff: Ok(diff_env(&baseline.env, &envp)),
      envp: Arc::new(Ok(envp)),
      has_dash_env,
      cred: Ok(Cred::default()),
      interpreter: None,
      fdinfo: Arc::new(FileDescriptorInfoCollection::default()),
      result: 0,
      timestamp: chrono::Local::now(),
      parent: None,
      cgroup: CgroupInfo::NotCollected,
    }
  }

  fn baseline(env: BTreeMap<OutputMsg, OutputMsg>) -> BaselineInfo {
    BaselineInfo {
      cwd: msg("/work"),
      env,
      fdinfo: FileDescriptorInfoCollection::default(),
    }
  }

  #[test]
  fn diff_env_commandline_separates_dash_filename_when_env_is_unchanged() {
    let env = BTreeMap::from([(msg("UNCHANGED"), msg("value"))]);
    let baseline = baseline(env.clone());
    let event = filename_event(&baseline, env, "--ignore-signal");

    let commandline = exec_commandline(
      &event,
      &baseline,
      &ModifierArgs::default(),
      RuntimeModifier::default(),
      false,
      SupportedShell::Bash,
    );

    assert_eq!(commandline.to_string(), "env -- --ignore-signal");
  }

  #[test]
  fn full_env_commandline_separates_dash_filename_when_env_is_empty() {
    let baseline = baseline(BTreeMap::new());
    let event = filename_event(&baseline, BTreeMap::new(), "--ignore-signal");

    let commandline = exec_commandline(
      &event,
      &baseline,
      &ModifierArgs::default(),
      RuntimeModifier::default(),
      true,
      SupportedShell::Bash,
    );

    assert_eq!(commandline.to_string(), "env -i -- --ignore-signal");
  }

  #[test]
  fn empty_env_commandlines_do_not_separate_non_dash_filename() {
    let baseline = baseline(BTreeMap::new());
    let event = filename_event(&baseline, BTreeMap::new(), "/bin/false");

    let diff_env = exec_commandline(
      &event,
      &baseline,
      &ModifierArgs::default(),
      RuntimeModifier::default(),
      false,
      SupportedShell::Bash,
    );
    let full_env = exec_commandline(
      &event,
      &baseline,
      &ModifierArgs::default(),
      RuntimeModifier::default(),
      true,
      SupportedShell::Bash,
    );

    assert_eq!(diff_env.to_string(), "env /bin/false");
    assert_eq!(full_env.to_string(), "env -i /bin/false");
  }
}
