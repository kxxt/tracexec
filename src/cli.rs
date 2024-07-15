use std::{borrow::Cow, io::stdout, num::ParseFloatError, path::PathBuf};

use clap::{CommandFactory, Parser, Subcommand};
use config::{Config, ExitHandling};
use tracing::debug;

use crate::{tracer::state::BreakPoint, tui::app::AppLayout};

use self::{
  args::{LogModeArgs, ModifierArgs, TracerEventArgs},
  options::{ActivePane, Color},
};

pub mod args;
pub mod config;
pub mod options;
#[cfg(test)]
mod test;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
  #[arg(long, default_value_t = Color::Auto, help = "Control whether colored output is enabled. This flag has no effect on TUI mode.")]
  pub color: Color,
  #[arg(
    short = 'C',
    long,
    help = "Change current directory to this path before doing anything"
  )]
  pub cwd: Option<PathBuf>,
  #[arg(
    short,
    long,
    help = "Run as user. This option is only available when running tracexec as root"
  )]
  pub user: Option<String>,
  #[clap(subcommand)]
  pub cmd: CliCommand,
}

#[derive(Subcommand, Debug)]
pub enum CliCommand {
  #[clap(about = "Run tracexec in logging mode")]
  Log {
    #[arg(last = true, required = true, help = "command to be executed")]
    cmd: Vec<String>,
    #[clap(flatten)]
    tracing_args: LogModeArgs,
    #[clap(flatten)]
    modifier_args: ModifierArgs,
    #[clap(flatten)]
    tracer_event_args: TracerEventArgs,
    #[clap(
      short,
      long,
      help = "Output, stderr by default. A single hyphen '-' represents stdout."
    )]
    output: Option<PathBuf>,
  },
  #[clap(about = "Run tracexec in TUI mode, stdin/out/err are redirected to /dev/null by default")]
  Tui {
    #[arg(last = true, required = true, help = "command to be executed")]
    cmd: Vec<String>,
    #[clap(flatten)]
    modifier_args: ModifierArgs,
    #[clap(flatten)]
    tracer_event_args: TracerEventArgs,
    #[clap(
      long,
      short,
      help = "Allocate a pseudo terminal and show it alongside the TUI"
    )]
    tty: bool,
    #[clap(long, short, help = "Keep the event list scrolled to the bottom")]
    follow: bool,
    #[clap(
      long,
      help = "Instead of waiting for the root child to exit, terminate when the TUI exits",
      conflicts_with = "kill_on_exit"
    )]
    terminate_on_exit: bool,
    #[clap(
      long,
      help = "Instead of waiting for the root child to exit, kill when the TUI exits"
    )]
    kill_on_exit: bool,
    #[clap(
      long,
      short = 'A',
      help = "Set the default active pane to use when TUI launches",
      requires = "tty"
    )]
    active_pane: Option<ActivePane>,
    #[clap(
      long,
      short = 'L',
      help = "Set the layout of the TUI when it launches",
      requires = "tty"
    )]
    layout: Option<AppLayout>,
    #[clap(
      long,
      short = 'F',
      help = "Set the frame rate of the TUI (60 by default)",
      value_parser = frame_rate_parser
    )]
    frame_rate: Option<f64>,
    #[clap(
      long,
      short = 'D',
      help = "Set the default external command to run when using \"Detach, Stop and Run Command\" feature in Hit Manager"
    )]
    default_external_command: Option<String>,
    #[clap(
      long = "add-breakpoint",
      short = 'b',
      value_parser = breakpoint_parser,
      help = "Add a new breakpoint to the tracer. This option can be used multiple times. The format is <syscall-stop>:<pattern-type>:<pattern>, where syscall-stop can be sysenter or sysexit, pattern-type can be argv-regex, in-filename or exact-filename. For example, sysexit:in-filename:/bash",
    )]
    breakpoints: Vec<BreakPoint>,
  },
  #[clap(about = "Generate shell completions for tracexec")]
  GenerateCompletions {
    #[arg(required = true, help = "The shell to generate completions for")]
    shell: clap_complete::Shell,
  },
}

#[derive(thiserror::Error, Debug)]
enum ParseFrameRateError {
  #[error("Failed to parse frame rate {0} as a floating point number")]
  ParseFloatError(ParseFloatError),
  #[error("Invalid frame rate")]
  InvalidFrameRate,
  #[error("Frame rate too low, must be at least 5.0")]
  FrameRateTooLow,
}

impl From<ParseFloatError> for ParseFrameRateError {
  fn from(e: ParseFloatError) -> Self {
    Self::ParseFloatError(e)
  }
}

fn frame_rate_parser(s: &str) -> Result<f64, ParseFrameRateError> {
  let v = s.parse::<f64>()?;
  if v < 0.0 || v.is_nan() || v.is_infinite() {
    Err(ParseFrameRateError::InvalidFrameRate)
  } else if v < 5.0 {
    Err(ParseFrameRateError::FrameRateTooLow)
  } else {
    Ok(v)
  }
}

fn breakpoint_parser(s: &str) -> Result<BreakPoint, Cow<'static, str>> {
  BreakPoint::try_from(s)
}

impl Cli {
  pub fn generate_completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, env!("CARGO_CRATE_NAME"), &mut stdout())
  }

  pub fn merge_config(&mut self, config: Config) {
    debug!("Merging config: {config:?}");
    match &mut self.cmd {
      CliCommand::Log {
        tracing_args,
        modifier_args,
        ..
      } => {
        if let Some(c) = config.modifier {
          modifier_args.merge_config(c);
        }
        if let Some(c) = config.log {
          tracing_args.merge_config(c);
        }
      }
      CliCommand::Tui {
        modifier_args,
        follow,
        terminate_on_exit,
        kill_on_exit,
        active_pane,
        layout,
        frame_rate,
        default_external_command,
        ..
      } => {
        if let Some(c) = config.modifier {
          modifier_args.merge_config(c);
        }
        if let Some(c) = config.tui {
          if active_pane.is_none() {
            *active_pane = c.active_pane;
          }
          if layout.is_none() {
            *layout = c.layout;
          }
          if default_external_command.is_none() {
            *default_external_command = c.default_external_command;
          }
          *frame_rate = frame_rate.or(c.frame_rate);
          *follow |= c.follow.unwrap_or_default();
          if (!*terminate_on_exit) && (!*kill_on_exit) {
            match c.exit_handling {
              Some(ExitHandling::Kill) => *kill_on_exit = true,
              Some(ExitHandling::Terminate) => *terminate_on_exit = true,
              _ => (),
            }
          }
        }
      }
      _ => (),
    }
  }
}
