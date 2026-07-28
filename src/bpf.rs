use std::{
  mem::MaybeUninit,
  process,
  sync::{
    Arc,
    atomic::Ordering,
  },
};

use enumflags2::BitFlag;
use nix::unistd::User;
use signal_hook::consts::signal::*;
use tokio::{
  sync::mpsc::{
    self,
  },
  task::spawn_blocking,
};
use tracexec_backend_ebpf::tracer::BuildEbpfTracer;
use tracexec_core::{
  cli::{
    Cli,
    EbpfCommand,
    args::TuiModeArgs,
    options::Color,
  },
  event::TracerEventDetailsKind,
  export::ExporterMetadata,
  proc::BaselineInfo,
  tracer::{
    TracerBuilder,
    TracerMode,
  },
};
use tracexec_tui::{
  self,
  app::App,
  theme::current_theme,
};

use crate::run_mode;

fn should_allocate_tui_pty(follow_forks: bool, tui_args: &TuiModeArgs) -> bool {
  follow_forks && tui_args.tty()
}

pub async fn main(
  command: EbpfCommand,
  user: Option<User>,
  color: Color,
  tracee_env: Option<tracexec_core::elevate::EnvVars>,
  tracexec_override_env: Option<tracexec_core::elevate::EnvVars>,
) -> color_eyre::Result<()> {
  let obj = Box::leak(Box::new(MaybeUninit::uninit()));
  match command {
    EbpfCommand::Log {
      cmd,
      output,
      modifier_args,
      log_args,
    } => {
      let modifier_args = modifier_args.processed();
      let baseline = Arc::new(BaselineInfo::new_with_env(tracee_env.as_deref())?);
      let output = Cli::get_output(output, color)?;
      let tracer = TracerBuilder::new()
        .mode(TracerMode::Log {
          foreground: log_args.foreground(),
        })
        .filter(TracerEventDetailsKind::empty())
        .baseline(baseline)
        .user(user)
        .tracee_env(tracee_env)
        .tracexec_override_env(tracexec_override_env)
        .modifier(modifier_args)
        .printer_from_cli(&log_args)
        .build_ebpf();
      let running_tracer = tracer.spawn(&cmd, obj, Some(output))?;
      running_tracer.run_until_exit()?;
      Ok(())
    }
    EbpfCommand::Tui {
      cmd,
      modifier_args,
      tracer_event_args,
      tui_args,
    } => {
      let executable_path = std::env::current_exe()?;
      run_mode::initialize_tui(&tui_args, &executable_path)?;
      let follow_forks = !cmd.is_empty();
      let tty = should_allocate_tui_pty(follow_forks, &tui_args);
      tui_args.validate_pty_options(tty)?;
      let modifier_args = modifier_args.processed();
      let (baseline, tracer_mode, pty_master) = run_mode::setup_tui_io(tty, tracee_env.as_deref())?;
      let baseline = Arc::new(baseline);
      let frame_rate = tui_args.frame_rate.unwrap_or(60.);
      let log_args = run_mode::tui_log_args();
      let mut app = App::new(
        None,
        &log_args,
        &modifier_args,
        tui_args,
        baseline.clone(),
        pty_master,
        current_theme(),
      )?;
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      // let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = TracerBuilder::new()
        .mode(tracer_mode)
        .baseline(baseline)
        .modifier(modifier_args)
        .printer_from_cli(&log_args)
        .filter(tracer_event_args.filter()?)
        .user(user)
        .tracee_env(tracee_env)
        .tracexec_override_env(tracexec_override_env)
        .tracer_tx(tracer_tx)
        .build_ebpf();
      let running_tracer = tracer.spawn(&cmd, obj, None)?;
      let should_exit = running_tracer.should_exit.clone();
      let tracer_thread = spawn_blocking(move || running_tracer.run_until_exit());
      let mut tui = tracexec_tui::Tui::new()?.frame_rate(frame_rate);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit()?;
      tracexec_tui::restore_tui()?;
      if !follow_forks {
        should_exit.store(true, Ordering::Relaxed);
      }
      tracer_thread.await??;
      Ok(())
    }
    EbpfCommand::Collect {
      cmd,
      modifier_args,
      exporter_args,
      format,
      output,
      foreground,
      no_foreground,
    } => {
      let modifier_args = modifier_args.processed();
      let baseline = Arc::new(BaselineInfo::new_with_env(tracee_env.as_deref())?);
      let output = Cli::get_output(output, color)?;
      let log_args = run_mode::collect_log_args(foreground, no_foreground);
      let (tx, rx) = mpsc::unbounded_channel();
      let tracer = TracerBuilder::new()
        .mode(TracerMode::Log {
          foreground: log_args.foreground(),
        })
        .modifier(modifier_args)
        .baseline(baseline.clone())
        .printer_from_cli(&log_args)
        .tracer_tx(tx)
        .user(user)
        .tracee_env(tracee_env)
        .tracexec_override_env(tracexec_override_env)
        .build_ebpf();
      let running_tracer = tracer.spawn(&cmd, obj, None)?;
      let should_exit = running_tracer.should_exit.clone();
      let tracer_thread = spawn_blocking(move || running_tracer.run_until_exit());
      let meta = ExporterMetadata {
        baseline: baseline.clone(),
        exporter_args,
      };
      run_mode::spawn_signal_handler([SIGTERM, SIGINT], move |_| {
        should_exit.store(true, Ordering::Relaxed);
      })?;
      let exit_code = run_mode::run_exporter(format, output, meta, rx).await?;
      tracer_thread.await??;
      process::exit(exit_code);
    }
  }
}

#[cfg(test)]
mod tests {
  use clap::Parser;
  use test_that::prelude::*;
  use tracexec_core::cli::CliCommand;

  use super::*;

  #[test]
  fn tui_pty_is_only_allocated_when_following_a_command() {
    let default_args = TuiModeArgs::default();
    let no_tty_args = TuiModeArgs {
      no_tty: true,
      ..Default::default()
    };

    assert!(!should_allocate_tui_pty(false, &default_args));
    assert!(should_allocate_tui_pty(true, &default_args));
    assert!(!should_allocate_tui_pty(true, &no_tty_args));
  }

  #[test]
  fn tui_rejects_pty_options_when_command_is_empty() {
    // eBPF TUI with an empty command traces all execs and does not allocate
    // a pseudo terminal, even without `--no-tty`. Options that require one
    // must be rejected instead of being silently ignored by the TUI setup.
    for args in [
      vec!["tracexec", "ebpf", "tui", "--active-pane", "terminal"],
      vec!["tracexec", "ebpf", "tui", "--scrollback-lines", "2000"],
    ] {
      let cli = Cli::try_parse_from(&args).unwrap();
      let CliCommand::Ebpf {
        command: EbpfCommand::Tui { cmd, tui_args, .. },
      } = cli.cmd
      else {
        panic!("expected ebpf tui command");
      };
      assert!(cmd.is_empty());
      let tty = should_allocate_tui_pty(!cmd.is_empty(), &tui_args);
      assert!(!tty);
      assert_that!(tui_args.validate_pty_options(tty), err(anything()));
    }
  }

  #[test]
  fn tui_accepts_non_pty_options_with_empty_command() {
    // `--active-pane events` matches the pane the TUI falls back to without
    // a pseudo terminal, and `--layout` is applied regardless, so both stay
    // valid when the eBPF command is empty.
    for args in [
      vec!["tracexec", "ebpf", "tui", "--active-pane", "events"],
      vec!["tracexec", "ebpf", "tui", "--layout", "vertical"],
    ] {
      let cli = Cli::try_parse_from(&args).unwrap();
      let CliCommand::Ebpf {
        command: EbpfCommand::Tui { cmd, tui_args, .. },
      } = cli.cmd
      else {
        panic!("expected ebpf tui command");
      };
      assert!(cmd.is_empty());
      let tty = should_allocate_tui_pty(!cmd.is_empty(), &tui_args);
      assert!(!tty);
      tui_args.validate_pty_options(tty).unwrap();
    }
  }

  #[test]
  fn tui_accepts_pty_options_when_following_a_command() {
    let cli = Cli::try_parse_from([
      "tracexec",
      "ebpf",
      "tui",
      "--active-pane",
      "terminal",
      "--",
      "sh",
    ])
    .unwrap();
    let CliCommand::Ebpf {
      command: EbpfCommand::Tui { cmd, tui_args, .. },
    } = cli.cmd
    else {
      panic!("expected ebpf tui command");
    };
    assert_eq!(cmd, vec!["sh"]);
    let tty = should_allocate_tui_pty(!cmd.is_empty(), &tui_args);
    assert!(tty);
    tui_args.validate_pty_options(tty).unwrap();
  }
}
