use std::borrow::Cow;

use itertools::{
  Itertools,
  chain,
};
use nix::fcntl::OFlag;
use ratatui::{
  style::Styled,
  text::{
    Line,
    Span,
  },
};
use tracexec_core::{
  cli::args::ModifierArgs,
  event::{
    EventStatus,
    ExecEvent,
    RuntimeModifier,
    TracerEventDetails,
    TracerEventMessage,
  },
  proc::{
    BaselineInfo,
    FileDescriptorInfoCollection,
  },
  timestamp::Timestamp,
};

use crate::{
  action::CopyTarget,
  event::private::Sealed,
  event_line::{
    EventLine,
    Mask,
  },
  output::OutputMsgTuiExt,
  theme::{
    Theme,
    current_theme,
  },
};

mod private {
  use tracexec_core::event::TracerEventDetails;

  pub trait Sealed {}

  impl Sealed for TracerEventDetails {}
}

pub trait TracerEventDetailsTuiExt: Sealed {
  fn to_tui_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    rt_modifier: RuntimeModifier,
    event_status: Option<EventStatus>,
    theme: &Theme,
  ) -> Line<'static>;

  #[allow(clippy::too_many_arguments)]
  fn to_event_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    rt_modifier: RuntimeModifier,
    event_status: Option<EventStatus>,
    enable_mask: bool,
    extra_prefix: Option<Span<'static>>,
    full_env: bool,
    theme: &Theme,
  ) -> EventLine;

  fn text_for_copy<'a>(
    &'a self,
    baseline: &BaselineInfo,
    target: CopyTarget,
    modifier_args: &ModifierArgs,
    rt_modifier: RuntimeModifier,
  ) -> Cow<'a, str>;
}

impl TracerEventDetailsTuiExt for TracerEventDetails {
  fn to_tui_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    rt_modifier: RuntimeModifier,
    event_status: Option<EventStatus>,
    theme: &Theme,
  ) -> Line<'static> {
    self
      .to_event_line(
        baseline,
        cmdline_only,
        modifier,
        rt_modifier,
        event_status,
        false,
        None,
        false,
        theme,
      )
      .line
  }

  /// Convert the event to a EventLine
  ///
  /// This method is resource intensive and the caller should cache the result
  #[allow(clippy::too_many_arguments)]
  fn to_event_line(
    &self,
    baseline: &BaselineInfo,
    cmdline_only: bool,
    modifier: &ModifierArgs,
    rt_modifier: RuntimeModifier,
    event_status: Option<EventStatus>,
    enable_mask: bool,
    extra_prefix: Option<Span<'static>>,
    full_env: bool,
    theme: &Theme,
  ) -> EventLine {
    fn handle_stdio_fd(
      fd: i32,
      baseline: &BaselineInfo,
      curr: &FileDescriptorInfoCollection,
      spans: &mut Vec<Span>,
      theme: &Theme,
    ) {
      let (fdstr, redir) = match fd {
        0 => (" 0", "<"),
        1 => (" 1", ">"),
        2 => (" 2", "2>"),
        _ => unreachable!(),
      };

      let space: Span = " ".into();
      let fdinfo_orig = baseline.fdinfo.get(fd).unwrap();
      if let Some(fdinfo) = curr.get(fd) {
        if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
          // stdio fd will be closed
          spans.push(fdstr.set_style(theme.cloexec_fd_in_cmdline));
          spans.push(">&-".set_style(theme.cloexec_fd_in_cmdline));
        } else if fdinfo.not_same_file_as(fdinfo_orig) {
          spans.push(space.clone());
          spans.push(redir.set_style(theme.modified_fd_in_cmdline));
          spans.push(
            fdinfo
              .path
              .bash_escaped_with_style(theme.modified_fd_in_cmdline, theme),
          );
        }
      } else {
        // stdio fd is closed
        spans.push(fdstr.set_style(theme.cloexec_fd_in_cmdline));
        spans.push(">&-".set_style(theme.removed_fd_in_cmdline));
      }
    }

    let mut env_range = None;
    let mut cwd_range = None;

    let rt_modifier_effective = if enable_mask {
      // Enable all modifiers so that the mask can be toggled later
      RuntimeModifier::default()
    } else {
      rt_modifier
    };

    let ts_formatter = |ts: Timestamp| {
      if modifier.timestamp {
        let fmt = modifier.inline_timestamp_format.as_deref().unwrap();
        Some(Span::styled(
          format!("{} ", ts.format(fmt)),
          theme.inline_timestamp,
        ))
      } else {
        None
      }
    };

    let mut line = match self {
      Self::Info(TracerEventMessage {
        msg,
        pid,
        timestamp,
      }) => chain!(
        extra_prefix,
        timestamp.and_then(ts_formatter),
        pid
          .map(|p| [p.to_string().set_style(theme.pid_in_msg)])
          .unwrap_or_default(),
        ["[info]".set_style(theme.tracer_info)],
        [": ".into(), msg.clone().set_style(theme.tracer_info)]
      )
      .collect(),
      Self::Warning(TracerEventMessage {
        msg,
        pid,
        timestamp,
      }) => chain!(
        extra_prefix,
        timestamp.and_then(ts_formatter),
        pid
          .map(|p| [p.to_string().set_style(theme.pid_in_msg)])
          .unwrap_or_default(),
        ["[warn]".set_style(theme.tracer_warning)],
        [": ".into(), msg.clone().set_style(theme.tracer_warning)]
      )
      .collect(),
      Self::Error(TracerEventMessage {
        msg,
        pid,
        timestamp,
      }) => chain!(
        extra_prefix,
        timestamp.and_then(ts_formatter),
        pid
          .map(|p| [p.to_string().set_style(theme.pid_in_msg)])
          .unwrap_or_default(),
        ["error".set_style(theme.tracer_error)],
        [": ".into(), msg.clone().set_style(theme.tracer_error)]
      )
      .collect(),
      Self::NewChild {
        ppid,
        pcomm,
        pid,
        timestamp,
      } => [
        extra_prefix,
        ts_formatter(*timestamp),
        Some(ppid.to_string().set_style(theme.pid_success)),
        event_status.map(|s| <&'static str>::from(s).into()),
        Some(format!("<{pcomm}>").set_style(theme.comm)),
        Some(": ".into()),
        Some("new child ".set_style(theme.tracer_event)),
        Some(pid.to_string().set_style(theme.new_child_pid)),
      ]
      .into_iter()
      .flatten()
      .collect(),
      Self::Exec(exec) => {
        let ExecEvent {
          pid,
          cwd,
          comm,
          filename,
          argv,
          interpreter: _,
          env_diff,
          result,
          fdinfo,
          envp,
          ..
        } = exec.as_ref();
        let mut spans = extra_prefix
          .into_iter()
          .chain(ts_formatter(exec.timestamp))
          .collect_vec();
        if !cmdline_only {
          spans.extend(
            [
              Some(pid.to_string().set_style(if *result == 0 {
                theme.pid_success
              } else if *result == (-nix::libc::ENOENT) as i64 {
                theme.pid_enoent
              } else {
                theme.pid_failure
              })),
              event_status.map(|s| <&'static str>::from(s).into()),
              Some(format!("<{comm}>").set_style(theme.comm)),
              Some(": ".into()),
              Some("env".set_style(theme.tracer_event)),
            ]
            .into_iter()
            .flatten(),
          )
        } else {
          spans.push("env".set_style(theme.tracer_event));
        };
        let space: Span = " ".into();

        // Handle argv[0]
        let _ = argv.as_deref().inspect(|v| {
          v.first().inspect(|&arg0| {
            if filename != arg0 {
              spans.push(space.clone());
              spans.push("-a ".set_style(theme.arg0));
              spans.push(arg0.bash_escaped_with_style(theme.arg0, theme));
            }
          });
        });
        // Handle cwd
        if cwd != &baseline.cwd && rt_modifier_effective.show_cwd {
          let range_start = spans.len();
          spans.push(space.clone());
          spans.push("-C ".set_style(theme.cwd));
          spans.push(cwd.bash_escaped_with_style(theme.cwd, theme));
          cwd_range = Some(range_start..(spans.len()))
        }
        if rt_modifier_effective.show_env {
          env_range = Some((spans.len(), 0));
          if !full_env {
            if let Ok(env_diff) = env_diff {
              // Handle env diff
              for k in env_diff.removed.iter() {
                spans.push(space.clone());
                spans.push("-u ".set_style(theme.deleted_env_var));
                spans.push(k.bash_escaped_with_style(theme.deleted_env_var, theme));
              }
              if env_diff.need_env_argument_separator() {
                spans.push(space.clone());
                spans.push("--".into());
              }
              for (k, v) in env_diff.added.iter() {
                // Added env vars
                spans.push(space.clone());
                spans.push(k.bash_escaped_with_style(theme.added_env_var, theme));
                spans.push("=".set_style(theme.added_env_var));
                spans.push(v.bash_escaped_with_style(theme.added_env_var, theme));
              }
              for (k, v) in env_diff.modified.iter() {
                // Modified env vars
                spans.push(space.clone());
                spans.push(k.bash_escaped_with_style(theme.modified_env_var, theme));
                spans.push("=".set_style(theme.modified_env_var));
                spans.push(v.bash_escaped_with_style(theme.modified_env_var, theme));
              }
            }
          } else if let Ok(envp) = &**envp {
            spans.push(space.clone());
            spans.push("-i --".into()); // TODO: style
            for (k, v) in envp.iter() {
              spans.push(space.clone());
              spans.push(k.bash_escaped_with_style(theme.unchanged_env_key, theme));
              spans.push("=".set_style(theme.unchanged_env_key));
              spans.push(v.bash_escaped_with_style(theme.unchanged_env_val, theme));
            }
          }

          if let Some(r) = env_range.as_mut() {
            r.1 = spans.len();
          }
        }
        spans.push(space.clone());
        // Filename
        spans.push(filename.bash_escaped_with_style(theme.filename, theme));
        // Argv[1..]
        match argv.as_ref() {
          Ok(argv) => {
            for arg in argv.iter().skip(1) {
              spans.push(space.clone());
              spans.push(arg.bash_escaped_with_style(theme.argv, theme));
            }
          }
          Err(_) => {
            spans.push(space.clone());
            spans.push("[failed to read argv]".set_style(theme.inline_tracer_error));
          }
        }

        // Handle file descriptors
        if modifier.stdio_in_cmdline {
          for fd in 0..=2 {
            handle_stdio_fd(fd, baseline, fdinfo, &mut spans, theme);
          }
        }

        if modifier.fd_in_cmdline {
          for (&fd, fdinfo) in fdinfo.fdinfo.iter() {
            if fd < 3 {
              continue;
            }
            if fdinfo.flags.intersects(OFlag::O_CLOEXEC) {
              // Skip fds that will be closed upon exec
              continue;
            }
            spans.push(space.clone());
            spans.push(fd.to_string().set_style(theme.added_fd_in_cmdline));
            spans.push("<>".set_style(theme.added_fd_in_cmdline));
            spans.push(
              fdinfo
                .path
                .bash_escaped_with_style(theme.added_fd_in_cmdline, theme),
            )
          }
        }

        Line::default().spans(spans)
      }
      Self::TraceeExit {
        signal,
        exit_code,
        timestamp,
      } => chain!(
        ts_formatter(*timestamp),
        Some(format!("tracee exit: signal: {signal:?}, exit_code: {exit_code}").into())
      )
      .collect(),
      Self::TraceeSpawn { pid, timestamp } => chain!(
        ts_formatter(*timestamp),
        Some(format!("tracee spawned: {pid}").into())
      )
      .collect(),
    };
    let mut cwd_mask = None;
    let mut env_mask = None;
    if enable_mask {
      if let Some(range) = cwd_range {
        let mut mask = Mask::new(range);
        if !rt_modifier.show_cwd {
          mask.toggle(&mut line);
        }
        cwd_mask.replace(mask);
      }
      if let Some((start, end)) = env_range {
        let mut mask = Mask::new(start..end);
        if !rt_modifier.show_env {
          mask.toggle(&mut line);
        }
        env_mask.replace(mask);
      }
    }
    EventLine {
      line,
      cwd_mask,
      env_mask,
    }
  }

  fn text_for_copy<'a>(
    &'a self,
    baseline: &BaselineInfo,
    target: CopyTarget,
    modifier_args: &ModifierArgs,
    rt_modifier: RuntimeModifier,
  ) -> Cow<'a, str> {
    if CopyTarget::Line == target {
      return self
        .to_event_line(
          baseline,
          false,
          modifier_args,
          rt_modifier,
          None,
          false,
          None,
          false,
          current_theme(),
        )
        .to_string()
        .into();
    }
    // Other targets are only available for Exec events
    let Self::Exec(event) = self else {
      panic!("Copy target {target:?} is only available for Exec events");
    };
    let mut modifier_args = ModifierArgs::default();
    match target {
      CopyTarget::Commandline(_) => self
        .to_event_line(
          baseline,
          true,
          &modifier_args,
          Default::default(),
          None,
          false,
          None,
          false,
          current_theme(),
        )
        .to_string()
        .into(),
      CopyTarget::CommandlineWithFullEnv(_) => self
        .to_event_line(
          baseline,
          true,
          &modifier_args,
          Default::default(),
          None,
          false,
          None,
          true,
          current_theme(),
        )
        .to_string()
        .into(),
      CopyTarget::CommandlineWithStdio(_) => {
        modifier_args.stdio_in_cmdline = true;
        self
          .to_event_line(
            baseline,
            true,
            &modifier_args,
            Default::default(),
            None,
            false,
            None,
            false,
            current_theme(),
          )
          .to_string()
          .into()
      }
      CopyTarget::CommandlineWithFds(_) => {
        modifier_args.fd_in_cmdline = true;
        modifier_args.stdio_in_cmdline = true;
        self
          .to_event_line(
            baseline,
            true,
            &modifier_args,
            Default::default(),
            None,
            false,
            None,
            false,
            current_theme(),
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
      CopyTarget::Argv => Self::argv_to_string(&event.argv).into(),
      CopyTarget::Filename => Cow::Borrowed(event.filename.as_ref()),
      CopyTarget::SyscallResult => event.result.to_string().into(),
      CopyTarget::Line => unreachable!(),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    sync::Arc,
  };

  use nix::{
    errno::Errno,
    fcntl::OFlag,
    unistd::Pid,
  };
  use tracexec_core::{
    cache::ArcStr,
    event::{
      ExecSyscall,
      OutputMsg,
    },
    proc::{
      CgroupInfo,
      Cred,
      FileDescriptorInfo,
      diff_env,
    },
    timestamp::{
      TimestampFormat,
      ts_from_boot_ns,
    },
  };

  use super::*;
  use crate::action::{
    CopyTarget,
    SupportedShell,
  };

  fn msg(value: &str) -> OutputMsg {
    OutputMsg::Ok(ArcStr::from(value))
  }

  fn fd(fd: i32, path: &str, ino: u64, flags: OFlag) -> FileDescriptorInfo {
    FileDescriptorInfo {
      fd,
      path: msg(path),
      pos: 0,
      flags,
      mnt_id: 1,
      ino,
      mnt: ArcStr::from("mnt"),
      extra: Vec::new(),
    }
  }

  fn fd_collection(
    entries: impl IntoIterator<Item = FileDescriptorInfo>,
  ) -> FileDescriptorInfoCollection {
    FileDescriptorInfoCollection {
      fdinfo: entries.into_iter().map(|info| (info.fd, info)).collect(),
    }
  }

  fn baseline() -> BaselineInfo {
    let mut env = BTreeMap::new();
    env.insert(msg("KEEP"), msg("same"));
    env.insert(msg("MODIFIED"), msg("old"));
    env.insert(msg("REMOVED"), msg("gone"));

    BaselineInfo {
      cwd: msg("/base"),
      env,
      fdinfo: fd_collection([
        fd(0, "/dev/stdin", 10, OFlag::empty()),
        fd(1, "/dev/stdout", 11, OFlag::empty()),
        fd(2, "/dev/stderr", 12, OFlag::empty()),
      ]),
    }
  }

  fn exec_details() -> TracerEventDetails {
    let baseline = baseline();
    let mut envp = BTreeMap::new();
    envp.insert(msg("KEEP"), msg("same"));
    envp.insert(msg("MODIFIED"), msg("new value"));
    envp.insert(msg("-ADDED"), msg("dash"));

    let fdinfo = fd_collection([
      fd(0, "/tmp/stdin", 20, OFlag::empty()),
      fd(1, "/tmp/stdout", 21, OFlag::empty()),
      fd(2, "/tmp/stderr", 22, OFlag::O_CLOEXEC),
      fd(5, "/tmp/fd5", 25, OFlag::empty()),
      fd(6, "/tmp/fd6", 26, OFlag::O_CLOEXEC),
    ]);
    let env_diff = diff_env(&baseline.env, &envp);

    TracerEventDetails::Exec(Box::new(ExecEvent {
      syscall: ExecSyscall::Execve,
      exec_pid: Pid::from_raw(42),
      pid: Pid::from_raw(42),
      cwd: msg("/work"),
      comm: ArcStr::from("echo"),
      filename: msg("/bin/echo"),
      argv: Arc::new(Ok(vec![msg("custom-argv0"), msg("hello world")])),
      envp: Arc::new(Ok(envp)),
      has_dash_env: false,
      cred: Ok(Cred::default()),
      interpreter: None,
      env_diff: Ok(env_diff),
      fdinfo: Arc::new(fdinfo),
      result: 0,
      timestamp: ts_from_boot_ns(10),
      parent: None,
      cgroup: CgroupInfo::NotCollected,
    }))
  }

  #[test]
  fn event_line_formats_messages_spawn_exit_and_new_child() {
    let baseline = baseline();
    let modifier = ModifierArgs::default();
    let theme = current_theme();
    let timestamp = ts_from_boot_ns(1);
    let cases = [
      TracerEventDetails::Info(TracerEventMessage {
        pid: Some(Pid::from_raw(1)),
        timestamp: None,
        msg: "info".to_string(),
      }),
      TracerEventDetails::Warning(TracerEventMessage {
        pid: Some(Pid::from_raw(2)),
        timestamp: None,
        msg: "warn".to_string(),
      }),
      TracerEventDetails::Error(TracerEventMessage {
        pid: Some(Pid::from_raw(3)),
        timestamp: None,
        msg: "err".to_string(),
      }),
      TracerEventDetails::NewChild {
        timestamp,
        ppid: Pid::from_raw(10),
        pcomm: ArcStr::from("parent"),
        pid: Pid::from_raw(11),
      },
      TracerEventDetails::TraceeSpawn {
        pid: Pid::from_raw(20),
        timestamp,
      },
      TracerEventDetails::TraceeExit {
        timestamp,
        signal: None,
        exit_code: 7,
      },
    ];

    let rendered = cases
      .iter()
      .map(|event| {
        event
          .to_event_line(
            &baseline,
            false,
            &modifier,
            RuntimeModifier::default(),
            Some(EventStatus::ProcessRunning),
            true,
            Some("prefix ".into()),
            false,
            theme,
          )
          .to_string()
      })
      .collect::<Vec<_>>()
      .join("\n");

    assert!(rendered.contains("[info]: info"));
    assert!(rendered.contains("[warn]: warn"));
    assert!(rendered.contains("error: err"));
    assert!(rendered.contains("new child 11"));
    assert!(rendered.contains("tracee spawned: 20"));
    assert!(rendered.contains("tracee exit: signal: None, exit_code: 7"));
  }

  #[test]
  fn exec_event_line_formats_cmdline_masks_full_env_and_fds() {
    let baseline = baseline();
    let mut modifier = ModifierArgs {
      stdio_in_cmdline: true,
      fd_in_cmdline: true,
      ..Default::default()
    };
    modifier.timestamp = true;
    modifier.inline_timestamp_format =
      Some(TimestampFormat::try_new("%H:%M:%S".to_string()).unwrap());
    let event = exec_details();
    let theme = current_theme();

    let line = event.to_event_line(
      &baseline,
      false,
      &modifier,
      RuntimeModifier {
        show_env: false,
        show_cwd: false,
      },
      Some(EventStatus::ProcessRunning),
      true,
      Some("exec ".into()),
      false,
      theme,
    );
    assert!(line.cwd_mask.is_some());
    assert!(line.env_mask.is_some());

    let commandline = event
      .to_event_line(
        &baseline,
        true,
        &modifier,
        RuntimeModifier::default(),
        None,
        false,
        None,
        false,
        theme,
      )
      .to_string();
    assert!(commandline.contains("env -a custom-argv0"));
    assert!(commandline.contains("-C /work"));
    assert!(commandline.contains("-u REMOVED"));
    assert!(commandline.contains("--"));
    assert!(commandline.contains("-ADDED=dash"));
    assert!(commandline.contains("MODIFIED=$'new value'"));
    assert!(commandline.contains("/bin/echo $'hello world'"));
    assert!(commandline.contains("</tmp/stdin"));
    assert!(commandline.contains(">/tmp/stdout"));
    assert!(commandline.contains("2>&-"));
    assert!(commandline.contains("5<>/tmp/fd5"));

    let full_env = event
      .to_event_line(
        &baseline,
        true,
        &modifier,
        RuntimeModifier::default(),
        None,
        false,
        None,
        true,
        theme,
      )
      .to_string();
    assert!(full_env.contains("-i --"));
    assert!(full_env.contains("KEEP=same"));
  }

  #[test]
  fn text_for_copy_covers_exec_targets_and_error_variants() {
    let baseline = baseline();
    let event = exec_details();
    let modifier = ModifierArgs::default();

    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::Line,
          &modifier,
          RuntimeModifier::default()
        )
        .contains("/bin/echo")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::Commandline(SupportedShell::Bash),
          &modifier,
          RuntimeModifier::default()
        )
        .contains("custom-argv0")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::CommandlineWithFullEnv(SupportedShell::Sh),
          &modifier,
          RuntimeModifier::default()
        )
        .contains("-i --")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::CommandlineWithStdio(SupportedShell::Fish),
          &modifier,
          RuntimeModifier::default()
        )
        .contains("</tmp/stdin")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::CommandlineWithFds(SupportedShell::Bash),
          &modifier,
          RuntimeModifier::default()
        )
        .contains("5<>/tmp/fd5")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::Env,
          &modifier,
          RuntimeModifier::default()
        )
        .contains("\"KEEP\"=\"same\"")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::EnvDiff,
          &modifier,
          RuntimeModifier::default()
        )
        .contains("# Added:")
    );
    assert!(
      event
        .text_for_copy(
          &baseline,
          CopyTarget::Argv,
          &modifier,
          RuntimeModifier::default()
        )
        .contains("hello world")
    );
    assert_eq!(
      event.text_for_copy(
        &baseline,
        CopyTarget::Filename,
        &modifier,
        RuntimeModifier::default()
      ),
      "/bin/echo"
    );
    assert_eq!(
      event.text_for_copy(
        &baseline,
        CopyTarget::SyscallResult,
        &modifier,
        RuntimeModifier::default()
      ),
      "0"
    );

    let mut failing = match exec_details() {
      TracerEventDetails::Exec(exec) => *exec,
      _ => unreachable!(),
    };
    failing.argv = Arc::new(Err(Errno::EACCES));
    failing.envp = Arc::new(Err(Errno::EPERM));
    failing.env_diff = Err(Errno::EPERM);
    let failing = TracerEventDetails::Exec(Box::new(failing));
    assert!(
      failing
        .text_for_copy(
          &baseline,
          CopyTarget::Argv,
          &modifier,
          RuntimeModifier::default()
        )
        .contains("failed to read argv")
    );
    assert!(
      failing
        .text_for_copy(
          &baseline,
          CopyTarget::Env,
          &modifier,
          RuntimeModifier::default()
        )
        .contains("failed to read envp")
    );
    assert_eq!(
      failing.text_for_copy(
        &baseline,
        CopyTarget::EnvDiff,
        &modifier,
        RuntimeModifier::default()
      ),
      "[failed to read envp]"
    );
  }
}
