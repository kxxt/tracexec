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
mod printer;
mod proc;
mod pty;
mod regex;
#[cfg(feature = "seccomp-bpf")]
mod seccomp;
mod tracer;
mod tui;

use std::{io, os::unix::ffi::OsStrExt, process, sync::Arc};

use atoi::atoi;
use clap::Parser;
use cli::{
  args::TracerEventArgs,
  config::{Config, ConfigLoadError},
  options::ExportFormat,
  Cli,
};
use color_eyre::eyre::{bail, OptionExt};

use export::{JsonExecEvent, JsonMetaData};
use nix::unistd::{Uid, User};
use serde::Serialize;
use tokio::sync::mpsc;
use tui::app::PTracer;

use crate::{
  cli::{args::LogModeArgs, options::Color, CliCommand},
  event::{TracerEvent, TracerEventDetails, TracerMessage},
  log::initialize_panic_handler,
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
  // Seccomp-bpf ptrace behavior is changed on 4.8. I haven't tested on older kernels.
  let min_support_kver = (4, 8);
  if !is_current_kernel_greater_than(min_support_kver)? {
    log::warn!(
      "Current kernel version is not supported! Minimum supported kernel version is {}.{}.",
      min_support_kver.0,
      min_support_kver.1
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
      let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = Arc::new(tracer::Tracer::new(
        TracerMode::Log {
          foreground: tracing_args.foreground(),
        },
        tracing_args,
        modifier_args,
        ptrace_args,
        tracer_event_args,
        baseline,
        tracer_tx,
        user,
        req_tx,
      )?);
      let tracer_thread = tracer.spawn(cmd, Some(output), req_rx);
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
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = Arc::new(tracer::Tracer::new(
        tracer_mode,
        tracing_args.clone(),
        modifier_args.clone(),
        ptrace_args,
        tracer_event_args,
        baseline.clone(),
        tracer_tx,
        user,
        req_tx,
      )?);
      let baseline = Arc::new(baseline);
      let frame_rate = tui_args.frame_rate.unwrap_or(60.);
      let mut app = App::new(
        Some(PTracer {
          tracer: tracer.clone(),
          debugger_args,
        }),
        &tracing_args,
        &modifier_args,
        tui_args,
        baseline,
        pty_master,
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
      pretty,
      foreground,
      no_foreground,
    } => {
      let modifier_args = modifier_args.processed();
      let mut output = Cli::get_output(output, cli.color)?;
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
      let (tracer_tx, mut tracer_rx) = mpsc::unbounded_channel();
      let (req_tx, req_rx) = mpsc::unbounded_channel();
      let baseline = BaselineInfo::new()?;
      let tracer = Arc::new(tracer::Tracer::new(
        TracerMode::Log {
          foreground: tracing_args.foreground(),
        },
        tracing_args.clone(),
        modifier_args.clone(),
        ptrace_args,
        TracerEventArgs::all(),
        baseline.clone(),
        tracer_tx,
        user,
        req_tx,
      )?);
      let tracer_thread = tracer.spawn(cmd, None, req_rx);
      match format {
        ExportFormat::Json => {
          let mut json = export::Json {
            meta: JsonMetaData::new(baseline),
            events: Vec::new(),
          };
          loop {
            match tracer_rx.recv().await {
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::TraceeExit { exit_code, .. },
                ..
              })) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await??;
                serialize_json_to_output(&mut output, &json, pretty)?;
                output.write_all(&[b'\n'])?;
                output.flush()?;
                process::exit(exit_code);
              }
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::Exec(exec),
                id,
              })) => {
                json.events.push(JsonExecEvent::new(id, *exec));
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
        ExportFormat::JsonStream => {
          serialize_json_to_output(&mut output, &JsonMetaData::new(baseline), pretty)?;
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
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::Exec(exec),
                id,
              })) => {
                let json_event = JsonExecEvent::new(id, *exec);
                serialize_json_to_output(&mut output, &json_event, pretty)?;
                output.write_all(&[b'\n'])?;
                output.flush()?;
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
      }
    }
    CliCommand::GenerateCompletions { shell } => {
      Cli::generate_completions(shell);
    }
    #[cfg(feature = "ebpf")]
    CliCommand::Ebpf { command, cmd } => {
      // TODO: warn if --user is set when not follow-forks
      bpf::run(command, user, cmd, cli.color).await?;
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

pub fn serialize_json_to_output<W, T>(writer: W, value: &T, pretty: bool) -> serde_json::Result<()>
where
  W: io::Write,
  T: ?Sized + Serialize,
{
  if pretty {
    serde_json::ser::to_writer_pretty(writer, value)
  } else {
    serde_json::ser::to_writer(writer, value)
  }
}
