mod action;
mod arch;
mod cache;
mod cli;
mod cmdbuilder;
mod event;
mod log;
mod printer;
mod proc;
mod pty;
mod regex;
#[cfg(feature = "seccomp-bpf")]
mod seccomp;
mod tracer;
mod tui;

use std::{
  io::{stderr, stdout, BufWriter},
  os::unix::ffi::OsStrExt,
  process,
  sync::Arc,
};

use atoi::atoi;
use clap::Parser;
use cli::Cli;
use color_eyre::eyre::{bail, OptionExt};

use nix::unistd::{Uid, User};
use tokio::sync::mpsc;

use crate::{
  cli::{args::LogModeArgs, options::Color, CliCommand},
  event::{TracerEvent, TracerEventDetails, TracerMessage},
  log::initialize_panic_handler,
  printer::PrinterOut,
  proc::BaselineInfo,
  pty::{native_pty_system, PtySize, PtySystem},
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
  if let Some(cwd) = cli.cwd {
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
  // Seccomp-bpf ptrace behavior is changed on 4.8. I haven't tested on older kernels.
  let min_support_kver = (4, 8);
  if !is_current_kernel_greater_than(min_support_kver)? {
    log::warn!(
      "Current kernel version is not supported! Minimum supported kernel version is {}.{}.",
      min_support_kver.0,
      min_support_kver.1
    );
  }
  match cli.cmd {
    CliCommand::Log {
      cmd,
      tracing_args,
      modifier_args,
      tracer_event_args,
      output,
    } => {
      let modifier_args = modifier_args.processed();
      let output: Box<PrinterOut> = match output {
        None => Box::new(stderr()),
        Some(ref x) if x.as_os_str() == "-" => Box::new(stdout()),
        Some(path) => {
          let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
          if cli.color != Color::Always {
            // Disable color by default when output is file
            owo_colors::control::set_should_colorize(false);
          }
          Box::new(BufWriter::new(file))
        }
      };
      let baseline = BaselineInfo::new()?;
      let (tracer_tx, mut tracer_rx) = mpsc::unbounded_channel();
      let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = Arc::new(tracer::Tracer::new(
        TracerMode::Log,
        tracing_args,
        modifier_args,
        tracer_event_args,
        baseline,
        tracer_tx,
        user,
        req_tx,
      )?);
      let tracer_thread = tracer.spawn(cmd, Some(output), req_rx);
      tracer_thread.await??;
      loop {
        if let Some(TracerMessage::Event(TracerEvent {
          details: TracerEventDetails::TraceeExit { exit_code, .. },
          ..
        })) = tracer_rx.recv().await
        {
          process::exit(exit_code);
        }
      }
    }
    CliCommand::Tui {
      cmd,
      modifier_args,
      tracer_event_args,
      tty,
      terminate_on_exit,
      active_pane,
      kill_on_exit,
      layout,
      follow,
      frame_rate,
    } => {
      let modifier_args = modifier_args.processed();
      // Disable owo-colors when running TUI
      owo_colors::control::set_should_colorize(false);
      log::debug!(
        "should colorize: {}",
        owo_colors::control::should_colorize()
      );
      let (baseline, tracer_mode, pty_master) = if tty {
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
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = Arc::new(tracer::Tracer::new(
        tracer_mode,
        tracing_args.clone(),
        modifier_args.clone(),
        tracer_event_args,
        baseline.clone(),
        tracer_tx,
        user,
        req_tx,
      )?);
      let mut app = App::new(
        tracer.clone(),
        &tracing_args,
        &modifier_args,
        baseline,
        pty_master,
        active_pane,
        layout,
        follow,
      )?;
      let tracer_thread = tracer.spawn(cmd, None, req_rx);
      let mut tui = tui::Tui::new()?.frame_rate(frame_rate);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit(terminate_on_exit, kill_on_exit)?;
      tui::restore_tui()?;
      tracer_thread.await??;
    }
    CliCommand::GenerateCompletions { shell } => {
      Cli::generate_completions(shell);
    }
  }
  Ok(())
}

fn is_current_kernel_greater_than(min_support: (u32, u32)) -> color_eyre::Result<bool> {
  let utsname = nix::sys::utsname::uname()?;
  let kstr = utsname.release().as_bytes();
  let pos = kstr.iter().position(|&c| c != b'.' && !c.is_ascii_digit());
  let kver = if let Some(pos) = pos {
    let (s, _) = kstr.split_at(pos);
    s
  } else {
    kstr
  };
  let mut kvers = kver.split(|&c| c == b'.');
  let Some(major) = kvers.next().and_then(atoi::<u32>) else {
    bail!("Failed to parse kernel major ver!")
  };
  let Some(minor) = kvers.next().and_then(atoi::<u32>) else {
    bail!("Failed to parse kernel minor ver!")
  };
  Ok((major, minor) >= min_support)
}
