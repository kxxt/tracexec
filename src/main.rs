#![allow(
  clippy::future_not_send, // We are not a library for now.
)]

#[cfg(feature = "ebpf")]
mod bpf;
mod log;

use std::{
  process,
  sync::Arc,
};

use clap::Parser;
use color_eyre::eyre::{
  OptionExt,
  bail,
};
use futures::StreamExt;
use nix::unistd::{
  Uid,
  User,
};
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;
use tokio::sync::mpsc;
use tracexec_backend_ptrace::ptrace::BuildPtraceTracer;
use tracexec_core::{
  cli::{
    Cli,
    CliCommand,
    args::{
      LogModeArgs,
      TracerEventArgs,
    },
    config::{
      Config,
      ConfigLoadError,
    },
    options::{
      Color,
      ExportFormat,
    },
  },
  event::{
    TracerEvent,
    TracerEventDetails,
    TracerMessage,
  },
  export::{
    Exporter,
    ExporterMetadata,
  },
  is_current_kernel_ge,
  proc::BaselineInfo,
  pty::{
    PtySize,
    PtySystem,
    native_pty_system,
  },
  tracer::{
    TracerBuilder,
    TracerMode,
  },
};
use tracexec_exporter_json::{
  JsonExporter,
  JsonStreamExporter,
};
use tracexec_exporter_perfetto::PerfettoExporter;
use tracexec_tui::{
  app::{
    App,
    PTracer,
  },
  theme::current_theme,
};

use crate::log::initialize_panic_handler;

// Avoid using tokio::main macro because we may need to restore env vars,
// which is not completely safe inside a multithreaded process.
fn main() -> color_eyre::Result<()> {
  let mut cli = Cli::parse();

  // Handle --elevate early, before any environment-sensitive setup.
  // This re-execs the process with elevated privileges and does not return.
  if cli.elevate {
    tracexec_core::elevate::elevate_and_reexec()?;
    unreachable!();
  }

  // Restore saved environment from a previous --elevate invocation.
  // This must happen before logging, config loading, or any code that reads env vars.
  if !cli.restore_env.is_empty() {
    tracexec_core::elevate::restore_env_from_entries(&cli.restore_env)?;
  }

  // Apply project directory overrides from --elevate before anything
  // touches project_directory() (logging, config, themes).
  // The clap validation ensures that the three elevated_*_dir options are either all present or all absent.
  if let (Some(config_dir), Some(data_dir), Some(data_local_dir)) = (
    cli.elevated_config_dir.take(),
    cli.elevated_data_dir.take(),
    cli.elevated_data_local_dir.take(),
  ) {
    tracexec_core::cli::config::set_project_dir_overrides(config_dir, data_dir, data_local_dir);
  }

  if cli.color == Color::Auto && std::env::var_os("NO_COLOR").is_some() {
    // Respect NO_COLOR if --color=auto
    cli.color = Color::Never;
  }
  if cli.color == Color::Always {
    owo_colors::control::set_should_colorize(true);
    color_eyre::install()?;
  } else if cli.color == Color::Never {
    owo_colors::control::set_should_colorize(false);
  } else {
    color_eyre::install()?;
  }
  initialize_panic_handler();
  log::initialize_logging()?;
  log::debug!("Commandline args: {:?}", cli);

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(2)
    .enable_all()
    .build()?;
  runtime.block_on(async_main(cli))
}

async fn async_main(mut cli: Cli) -> color_eyre::Result<()> {
  if let Some(cwd) = &cli.cwd {
    std::env::set_current_dir(cwd)?;
  }
  let user = if let Some(user) = cli.user.as_deref() {
    if !Uid::effective().is_root() {
      bail!("--user option is only available when running tracexec as root!");
    }
    Some(User::from_name(user)?.ok_or_eyre("Failed to get user info")?)
  } else {
    None
  };
  // PTRACE_GET_SYSCALL_INFO requires at least linux 5.3.
  let min_support_kver = (5, 3);
  if !is_current_kernel_ge(min_support_kver)? {
    log::warn!(
      "Current kernel version is not supported! Minimum supported kernel version is {}.{}.",
      min_support_kver.0,
      min_support_kver.1
    );
    eprintln!(
      "Current kernel version is not supported! Minimum supported kernel version is {}.{}.",
      min_support_kver.0, min_support_kver.1
    );
  }
  if !cli.no_profile {
    match Config::load(cli.profile.clone()) {
      Ok(config) => cli.merge_config(config),
      Err(ConfigLoadError::NotFound) => (),
      Err(e) => Err(e)?,
    };
  }
  match cli.cmd {
    CliCommand::Log {
      cmd,
      tracing_args,
      modifier_args,
      ptrace_args,
      tracer_event_args,
      output,
    } => {
      let modifier_args = modifier_args.processed();
      let output = Cli::get_output(output, cli.color)?;
      let baseline = BaselineInfo::new()?;
      let (tracer_tx, mut tracer_rx) = mpsc::unbounded_channel();
      let (tracer, token) = TracerBuilder::new()
        .mode(TracerMode::Log {
          foreground: tracing_args.foreground(),
        })
        .modifier(modifier_args)
        .user(user)
        .tracer_tx(tracer_tx)
        .baseline(Arc::new(baseline))
        .filter(tracer_event_args.filter()?)
        .seccomp_bpf(ptrace_args.seccomp_bpf)
        .ptrace_blocking(ptrace_args.polling_interval.is_none_or(|v| v < 0))
        .ptrace_polling_delay(
          ptrace_args
            .polling_interval
            .filter(|&v| v > 0)
            .map(|v| v as u64),
        )
        .printer_from_cli(&tracing_args)
        .build_ptrace()?;
      let (_tracer, tracer_thread) = tracer.spawn(cmd, Some(output), token)?;
      loop {
        match tracer_rx.recv().await {
          Some(TracerMessage::Event(TracerEvent {
            details: TracerEventDetails::TraceeExit { exit_code, .. },
            ..
          })) => {
            tracing::debug!("Waiting for tracer thread to exit");
            tracer_thread.await??;
            process::exit(exit_code);
          }
          // channel closed abnormally.
          None | Some(TracerMessage::FatalError(_)) => {
            tracing::debug!("Waiting for tracer thread to exit");
            tracer_thread.await??;
            process::exit(1);
          }
          _ => (),
        }
      }
    }
    CliCommand::Tui {
      cmd,
      modifier_args,
      ptrace_args,
      tracer_event_args,
      tui_args,
      debugger_args,
    } => {
      let executable_path = std::env::current_exe()?;
      tracexec_tui::theme::initialize(
        tui_args.theme_file.as_ref().map(|p| p.as_deref()),
        tui_args.theme.as_deref(),
        &executable_path,
        tui_args
          .theme_file
          .as_ref()
          .is_some_and(|f| f.is_from_cli()),
      )?;
      let modifier_args = modifier_args.processed();
      // Disable owo-colors when running TUI
      owo_colors::control::set_should_colorize(false);
      log::debug!(
        "should colorize: {}",
        owo_colors::control::should_colorize()
      );
      let (baseline, tracer_mode, pty_master) = if tui_args.tty {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
          rows: 24,
          cols: 80,
          pixel_width: 0,
          pixel_height: 0,
        })?;
        (
          BaselineInfo::with_pts(&pair.slave)?,
          TracerMode::Tui(Some(pair.slave)),
          Some(pair.master),
        )
      } else {
        (BaselineInfo::new()?, TracerMode::Tui(None), None)
      };
      let tracing_args = LogModeArgs {
        show_cmdline: false, // We handle cmdline in TUI
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        diff_env: true,
        ..Default::default()
      };
      let baseline = Arc::new(baseline);
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      let (tracer, token) = TracerBuilder::new()
        .mode(tracer_mode)
        .modifier(modifier_args.clone())
        .user(user)
        .tracer_tx(tracer_tx)
        .baseline(baseline.clone())
        .filter(tracer_event_args.filter()?)
        .seccomp_bpf(ptrace_args.seccomp_bpf)
        .ptrace_blocking(ptrace_args.polling_interval.is_none_or(|v| v < 0))
        .ptrace_polling_delay(
          ptrace_args
            .polling_interval
            .filter(|&v| v > 0)
            .map(|v| v as u64),
        )
        .printer_from_cli(&tracing_args)
        .build_ptrace()?;

      let frame_rate = tui_args.frame_rate.unwrap_or(60.);
      let (tracer, tracer_thread) = tracer.spawn(cmd, None, token)?;
      let mut app = App::new(
        Some(PTracer {
          tracer,
          debugger_args,
        }),
        &tracing_args,
        &modifier_args,
        tui_args,
        baseline,
        pty_master,
        current_theme(),
      )?;
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
      tracer_thread.await??;
    }
    CliCommand::Collect {
      cmd,
      format,
      output,
      modifier_args,
      ptrace_args,
      exporter_args,
      foreground,
      no_foreground,
    } => {
      let modifier_args = modifier_args.processed();
      let output = Cli::get_output(output, cli.color)?;
      let tracing_args = LogModeArgs {
        show_cmdline: false,
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        diff_env: false,
        foreground,
        no_foreground,
        ..Default::default()
      };
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      let baseline = Arc::new(BaselineInfo::new()?);
      let (tracer, token) = TracerBuilder::new()
        .mode(TracerMode::Log {
          foreground: tracing_args.foreground(),
        })
        .modifier(modifier_args)
        .user(user)
        .tracer_tx(tracer_tx)
        .baseline(baseline.clone())
        .filter(TracerEventArgs::all().filter()?)
        .seccomp_bpf(ptrace_args.seccomp_bpf)
        .ptrace_blocking(ptrace_args.polling_interval.is_none_or(|v| v < 0))
        .ptrace_polling_delay(
          ptrace_args
            .polling_interval
            .filter(|&v| v > 0)
            .map(|v| v as u64),
        )
        .printer_from_cli(&tracing_args)
        .build_ptrace()?;
      let (tracer, tracer_thread) = tracer.spawn(cmd, None, token)?;
      let signals = Signals::new([SIGTERM, SIGINT, SIGQUIT])?;
      tokio::spawn(async move {
        let mut signals = signals;
        while let Some(signal) = signals.next().await {
          match signal {
            SIGTERM | SIGINT => {
              tracer
                .request_termination()
                .expect("Failed to terminate tracer");
            }
            _ => unreachable!(),
          }
        }
      });
      let meta = ExporterMetadata {
        baseline,
        exporter_args,
      };
      match format {
        ExportFormat::Json => {
          let exporter = JsonExporter::new(output, meta, tracer_rx)?;
          let exit_code = exporter.run().await?;
          tracer_thread.await??;
          process::exit(exit_code);
        }
        ExportFormat::JsonStream => {
          let exporter = JsonStreamExporter::new(output, meta, tracer_rx)?;
          let exit_code = exporter.run().await?;
          tracer_thread.await??;
          process::exit(exit_code);
        }
        ExportFormat::Perfetto => {
          let exporter = PerfettoExporter::new(output, meta, tracer_rx)?;
          let exit_code = exporter.run().await?;
          tracer_thread.await??;
          process::exit(exit_code);
        }
      }
    }
    CliCommand::GenerateCompletions { shell } => {
      Cli::generate_completions(shell);
    }
    #[cfg(feature = "ebpf")]
    CliCommand::Ebpf { command } => {
      // TODO: warn if --user is set when not follow-forks
      bpf::main(command, user, cli.color).await?;
    }
  }
  Ok(())
}
