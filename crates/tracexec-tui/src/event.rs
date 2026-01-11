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
  theme::THEME,
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
  ) -> Line<'static>;

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
  ) -> EventLine {
    fn handle_stdio_fd(
      fd: i32,
      baseline: &BaselineInfo,
      curr: &FileDescriptorInfoCollection,
      spans: &mut Vec<Span>,
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
          spans.push(fdstr.set_style(THEME.cloexec_fd_in_cmdline));
          spans.push(">&-".set_style(THEME.cloexec_fd_in_cmdline));
        } else if fdinfo.not_same_file_as(fdinfo_orig) {
          spans.push(space.clone());
          spans.push(redir.set_style(THEME.modified_fd_in_cmdline));
          spans.push(
            fdinfo
              .path
              .bash_escaped_with_style(THEME.modified_fd_in_cmdline),
          );
        }
      } else {
        // stdio fd is closed
        spans.push(fdstr.set_style(THEME.cloexec_fd_in_cmdline));
        spans.push(">&-".set_style(THEME.removed_fd_in_cmdline));
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
          THEME.inline_timestamp,
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
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["[info]".set_style(THEME.tracer_info)],
        [": ".into(), msg.clone().set_style(THEME.tracer_info)]
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
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["[warn]".set_style(THEME.tracer_warning)],
        [": ".into(), msg.clone().set_style(THEME.tracer_warning)]
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
          .map(|p| [p.to_string().set_style(THEME.pid_in_msg)])
          .unwrap_or_default(),
        ["error".set_style(THEME.tracer_error)],
        [": ".into(), msg.clone().set_style(THEME.tracer_error)]
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
        Some(ppid.to_string().set_style(THEME.pid_success)),
        event_status.map(|s| <&'static str>::from(s).into()),
        Some(format!("<{pcomm}>").set_style(THEME.comm)),
        Some(": ".into()),
        Some("new child ".set_style(THEME.tracer_event)),
        Some(pid.to_string().set_style(THEME.new_child_pid)),
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
                THEME.pid_success
              } else if *result == (-nix::libc::ENOENT) as i64 {
                THEME.pid_enoent
              } else {
                THEME.pid_failure
              })),
              event_status.map(|s| <&'static str>::from(s).into()),
              Some(format!("<{comm}>").set_style(THEME.comm)),
              Some(": ".into()),
              Some("env".set_style(THEME.tracer_event)),
            ]
            .into_iter()
            .flatten(),
          )
        } else {
          spans.push("env".set_style(THEME.tracer_event));
        };
        let space: Span = " ".into();

        // Handle argv[0]
        let _ = argv.as_deref().inspect(|v| {
          v.first().inspect(|&arg0| {
            if filename != arg0 {
              spans.push(space.clone());
              spans.push("-a ".set_style(THEME.arg0));
              spans.push(arg0.bash_escaped_with_style(THEME.arg0));
            }
          });
        });
        // Handle cwd
        if cwd != &baseline.cwd && rt_modifier_effective.show_cwd {
          let range_start = spans.len();
          spans.push(space.clone());
          spans.push("-C ".set_style(THEME.cwd));
          spans.push(cwd.bash_escaped_with_style(THEME.cwd));
          cwd_range = Some(range_start..(spans.len()))
        }
        if rt_modifier_effective.show_env {
          env_range = Some((spans.len(), 0));
          if !full_env {
            if let Ok(env_diff) = env_diff {
              // Handle env diff
              for k in env_diff.removed.iter() {
                spans.push(space.clone());
                spans.push("-u ".set_style(THEME.deleted_env_var));
                spans.push(k.bash_escaped_with_style(THEME.deleted_env_var));
              }
              if env_diff.need_env_argument_separator() {
                spans.push(space.clone());
                spans.push("--".into());
              }
              for (k, v) in env_diff.added.iter() {
                // Added env vars
                spans.push(space.clone());
                spans.push(k.bash_escaped_with_style(THEME.added_env_var));
                spans.push("=".set_style(THEME.added_env_var));
                spans.push(v.bash_escaped_with_style(THEME.added_env_var));
              }
              for (k, v) in env_diff.modified.iter() {
                // Modified env vars
                spans.push(space.clone());
                spans.push(k.bash_escaped_with_style(THEME.modified_env_var));
                spans.push("=".set_style(THEME.modified_env_var));
                spans.push(v.bash_escaped_with_style(THEME.modified_env_var));
              }
            }
          } else if let Ok(envp) = &**envp {
            spans.push(space.clone());
            spans.push("-i --".into()); // TODO: style
            for (k, v) in envp.iter() {
              spans.push(space.clone());
              spans.push(k.bash_escaped_with_style(THEME.unchanged_env_key));
              spans.push("=".set_style(THEME.unchanged_env_key));
              spans.push(v.bash_escaped_with_style(THEME.unchanged_env_val));
            }
          }

          if let Some(r) = env_range.as_mut() {
            r.1 = spans.len();
          }
        }
        spans.push(space.clone());
        // Filename
        spans.push(filename.bash_escaped_with_style(THEME.filename));
        // Argv[1..]
        match argv.as_ref() {
          Ok(argv) => {
            for arg in argv.iter().skip(1) {
              spans.push(space.clone());
              spans.push(arg.bash_escaped_with_style(THEME.argv));
            }
          }
          Err(_) => {
            spans.push(space.clone());
            spans.push("[failed to read argv]".set_style(THEME.inline_tracer_error));
          }
        }

        // Handle file descriptors
        if modifier.stdio_in_cmdline {
          for fd in 0..=2 {
            handle_stdio_fd(fd, baseline, fdinfo, &mut spans);
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
            spans.push(fd.to_string().set_style(THEME.added_fd_in_cmdline));
            spans.push("<>".set_style(THEME.added_fd_in_cmdline));
            spans.push(
              fdinfo
                .path
                .bash_escaped_with_style(THEME.added_fd_in_cmdline),
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
