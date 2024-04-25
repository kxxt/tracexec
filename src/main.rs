mod arch;
mod cli;
mod cmdbuilder;
mod event;
mod inspect;
mod log;
mod printer;
mod proc;
mod ptrace;
mod pty;
#[cfg(feature = "seccomp-bpf")]
mod seccomp;
mod state;
mod tracer;
mod tui;

use std::{
  io::{stderr, stdout, BufWriter, Write},
  os::unix::ffi::OsStrExt,
  process, thread,
};

use atoi::atoi;
use clap::Parser;
use cli::Cli;
use color_eyre::eyre::{bail, OptionExt};

use nix::unistd::{Uid, User};
use tokio::sync::mpsc;

use crate::{
  cli::{CliCommand, Color, TracingArgs},
  event::TracerEvent,
  log::initialize_panic_handler,
  pty::{native_pty_system, PtySize, PtySystem},
  tracer::TracerMode,
  tui::event_list::EventListApp,
};

#[tokio::main]
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
  // TODO: separate output verbosity from log level
  // pretty_env_logger::formatted_builder()
  //     .filter_level(match (cli.quiet, cli.verbose) {
  //         // Don't follow RUST_LOG environment variable.
  //         (true, _) => log::LevelFilter::Error,
  //         (false, 0) => log::LevelFilter::Warn,
  //         (false, 1) => log::LevelFilter::Info,
  //         (false, 2) => log::LevelFilter::Debug,
  //         (false, _) => log::LevelFilter::Trace,
  //     })
  //     .init();
  log::trace!("Commandline args: {:?}", cli);
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
      output,
    } => {
      let output: Box<dyn Write + Send> = match output {
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
      let (tracer_tx, mut tracer_rx) = mpsc::unbounded_channel();
      let mut tracer = tracer::Tracer::new(
        TracerMode::Cli,
        tracing_args,
        modifier_args,
        Some(output),
        tracer_tx,
        user,
      )?;
      let tracer_thread = thread::Builder::new()
        .name("tracer".to_string())
        .spawn(move || tracer.start_root_process(cmd))?;
      tracer_thread.join().unwrap()?;
      loop {
        if let Some(TracerEvent::RootChildExit { exit_code, .. }) = tracer_rx.recv().await {
          process::exit(exit_code);
        }
      }
    }
    CliCommand::Tui {
      cmd,
      modifier_args,
      tty,
      terminate_on_exit,
      kill_on_exit,
    } => {
      let (tracer_mode, pty_master) = if tty {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
          rows: 24,
          cols: 80,
          pixel_width: 0,
          pixel_height: 0,
        })?;
        (TracerMode::Tui(Some(pair.slave)), Some(pair.master))
      } else {
        (TracerMode::Tui(None), None)
      };
      let tracing_args = TracingArgs {
        show_cmdline: true,
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        show_children: true,
        diff_env: true,
        ..Default::default()
      };
      let mut app = EventListApp::new(&tracing_args, &modifier_args, pty_master)?;
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      let mut tracer = tracer::Tracer::new(
        tracer_mode,
        tracing_args,
        modifier_args,
        None,
        tracer_tx,
        user,
      )?;
      let tracer_thread = thread::Builder::new()
        .name("tracer".to_string())
        .spawn(move || tracer.start_root_process(cmd))?;
      let mut tui = tui::Tui::new()?.frame_rate(30.0);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit(terminate_on_exit, kill_on_exit)?;
      tui::restore_tui()?;
      tracer_thread.join().unwrap()?;
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
