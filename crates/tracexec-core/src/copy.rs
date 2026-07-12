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
  baseline: &BaselineInfo,
  curr: &FileDescriptorInfoCollection,
  commandline: &mut Commandline,
) {
  let (fdstr, redir) = match fd {
    0 => (" 0", "<"),
    1 => (" 1", ">"),
    2 => (" 2", "2>"),
    _ => unreachable!(),
  };

  let fdinfo_orig = baseline.fdinfo.get(fd).unwrap();
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
        if env_diff.need_env_argument_separator() {
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
      commandline.push("-i --", CommandlinePartKind::Plain);
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
    for fd in 0..=2 {
      handle_stdio_fd(fd, baseline, fdinfo, &mut commandline);
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
      for (k, v) in env_diff.modified.iter() {
        result.push_str(&format!(
          "{}={}\n{}={}\n",
          k,
          baseline.env.get(k).unwrap(),
          k,
          v
        ));
      }
      result.push_str("# Removed:\n");
      for k in env_diff.removed.iter() {
        result.push_str(&format!("{}={}\n", k, baseline.env.get(k).unwrap()));
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
