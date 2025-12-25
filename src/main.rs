#![warn(
  clippy::all,
  // clippy::pedantic,
  clippy::nursery,
)]
#![allow(
  clippy::option_if_let_else,
  clippy::missing_const_for_fn,
  clippy::significant_drop_tightening,
  clippy::cognitive_complexity, // FIXME
  clippy::large_stack_frames, // In generated bpf skel, not really used to store on stack.
  clippy::future_not_send, // We are not a library for now.
  clippy::branches_sharing_code,
  clippy::non_send_fields_in_send_ty, // In bpf skel, maybe open an issue in libbpf-rs?
)]

mod action;
mod arch;
#[cfg(feature = "ebpf")]
mod bpf;
mod cache;
mod cli;
mod cmdbuilder;
mod event;
mod export;
mod log;
mod primitives;
mod printer;
mod proc;
mod ptrace;
mod pty;
mod seccomp;
mod timestamp;
mod tracee;
mod tracer;
mod tui;

use std::{os::unix::ffi::OsStrExt, process, sync::Arc};

use atoi::atoi;
use clap::Parser;
use cli::{
  Cli,
  args::TracerEventArgs,
  config::{Config, ConfigLoadError},
  options::ExportFormat,
};
use color_eyre::eyre::{OptionExt, bail};

use nix::unistd::{Uid, User};
use tokio::sync::mpsc;
use tracer::TracerBuilder;
use tui::app::PTracer;

use crate::{
  cli::{CliCommand, args::LogModeArgs, options::Color},
  event::{TracerEvent, TracerEventDetails, TracerMessage},
  export::{Exporter, ExporterMetadata, JsonExporter, JsonStreamExporter, PerfettoExporter},
  log::initialize_panic_handler,
  proc::BaselineInfo,
  pty::{PtySize, PtySystem, native_pty_system},
  tracer::TracerMode,
  tui::app::App,
};

#[tokio::main(worker_threads = 2)]
async fn main() -> color_eyre::Result<()> {
  let mut cli = Cli::parse();
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
      )?;
      let mut tui = tui::Tui::new()?.frame_rate(frame_rate);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit()?;
      tui::restore_tui()?;
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
      let (_tracer, tracer_thread) = tracer.spawn(cmd, None, token)?;
      let meta = ExporterMetadata { baseline, exporter_args };
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

fn is_current_kernel_ge(min_support: (u32, u32)) -> color_eyre::Result<bool> {
  let utsname = nix::sys::utsname::uname()?;
  let kstr = utsname.release().as_bytes();
  let pos = kstr.iter().position(|&c| c != b'.' && !c.is_ascii_digit());
  let kver = pos.map_or(kstr, |pos| kstr.split_at(pos).0);
  let mut kvers = kver.split(|&c| c == b'.');
  let Some(major) = kvers.next().and_then(atoi::<u32>) else {
    bail!("Failed to parse kernel major ver!")
  };
  let Some(minor) = kvers.next().and_then(atoi::<u32>) else {
    bail!("Failed to parse kernel minor ver!")
  };
  Ok((major, minor) >= min_support)
}
