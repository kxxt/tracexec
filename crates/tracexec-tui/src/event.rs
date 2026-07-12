use std::borrow::Cow;

use itertools::{
  Itertools,
  chain,
};
use ratatui::{
  style::{
    Style,
    Styled,
  },
  text::{
    Line,
    Span,
  },
};
use tracexec_core::{
  cli::args::ModifierArgs,
  copy::{
    self,
    CommandlinePartKind,
    OutputStatus,
    SupportedShell,
  },
  event::{
    EventStatus,
    ExecEvent,
    RuntimeModifier,
    TracerEventDetails,
  },
  proc::BaselineInfo,
  timestamp::Timestamp,
};

use crate::{
  action::CopyTarget,
  event::private::Sealed,
  event_line::{
    EventLine,
    Mask,
  },
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

fn commandline_part_style(
  kind: CommandlinePartKind,
  output_status: Option<OutputStatus>,
  theme: &Theme,
) -> Style {
  let style = match kind {
    CommandlinePartKind::Plain => Style::default(),
    CommandlinePartKind::TracerEvent => theme.tracer_event,
    CommandlinePartKind::Arg0 => theme.arg0,
    CommandlinePartKind::Cwd => theme.cwd,
    CommandlinePartKind::DeletedEnvVar => theme.deleted_env_var,
    CommandlinePartKind::AddedEnvVar => theme.added_env_var,
    CommandlinePartKind::ModifiedEnvVar => theme.modified_env_var,
    CommandlinePartKind::UnchangedEnvKey => theme.unchanged_env_key,
    CommandlinePartKind::UnchangedEnvVal => theme.unchanged_env_val,
    CommandlinePartKind::Filename => theme.filename,
    CommandlinePartKind::Argv => theme.argv,
    CommandlinePartKind::InlineTracerError => theme.inline_tracer_error,
    CommandlinePartKind::ModifiedFdInCommandline => theme.modified_fd_in_cmdline,
    CommandlinePartKind::RemovedFdInCommandline => theme.removed_fd_in_cmdline,
    CommandlinePartKind::CloexecFdInCommandline => theme.cloexec_fd_in_cmdline,
    CommandlinePartKind::AddedFdInCommandline => theme.added_fd_in_cmdline,
  };

  match output_status {
    Some(OutputStatus::PartialOk) => style.patch(theme.partial_ok),
    Some(OutputStatus::Err) => theme.inline_tracer_error,
    Some(OutputStatus::Ok) | None => style,
  }
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

    macro_rules! tracer_message_line {
      ($message:expr, $label:literal, $style:expr) => {{
        let msg_ref = $message;
        let style_val = $style;
        chain!(
          extra_prefix,
          msg_ref.timestamp.and_then(ts_formatter),
          msg_ref
            .pid
            .map(|pid| [pid.to_string().set_style(theme.pid_in_msg)])
            .unwrap_or_default(),
          [$label.set_style(style_val)],
          [": ".into(), msg_ref.msg.clone().set_style(style_val)]
        )
        .collect()
      }};
    }

    let mut line = match self {
      Self::Info(message) => tracer_message_line!(message, "[info]", theme.tracer_info),
      Self::Warning(message) => tracer_message_line!(message, "[warn]", theme.tracer_warning),
      Self::Error(message) => tracer_message_line!(message, "error", theme.tracer_error),
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
          comm,
          interpreter: _,
          result,
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
            ]
            .into_iter()
            .flatten(),
          )
        };
        let commandline = copy::exec_commandline(
          exec,
          baseline,
          modifier,
          rt_modifier_effective,
          full_env,
          SupportedShell::Bash,
        );
        let commandline_offset = spans.len();
        cwd_range = commandline
          .cwd_range
          .map(|range| (commandline_offset + range.start)..(commandline_offset + range.end));
        env_range = commandline.env_range.map(|range| {
          (
            commandline_offset + range.start,
            commandline_offset + range.end,
          )
        });
        spans.extend(commandline.parts.into_iter().map(|part| {
          Span::styled(
            part.text,
            commandline_part_style(part.kind, part.output_status, theme),
          )
        }));

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
    copy::text_for_copy(self, baseline, target, modifier_args, rt_modifier)
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
      TracerEventMessage,
    },
    proc::{
      CgroupInfo,
      Cred,
      FileDescriptorInfo,
      FileDescriptorInfoCollection,
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
      pos: 0.into(),
      flags: flags.into(),
      mnt_id: 1.into(),
      ino: ino.into(),
      mnt: ArcStr::from("mnt"),
      extra: Vec::new(),
    }
  }

  fn fd_collection(
    entries: impl IntoIterator<Item = FileDescriptorInfo>,
  ) -> FileDescriptorInfoCollection {
    FileDescriptorInfoCollection {
      fdinfo: entries.into_iter().map(|info| (info.fd, info)).collect(),
      error: None,
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
        CopyTarget::ArgvJoined,
        &modifier,
        RuntimeModifier::default()
      ),
      "custom-argv0 hello world"
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
    assert_eq!(
      failing.text_for_copy(
        &baseline,
        CopyTarget::ArgvJoined,
        &modifier,
        RuntimeModifier::default()
      ),
      "[failed to read argv]"
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
