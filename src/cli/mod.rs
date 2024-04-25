use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

use self::{
  args::{ModifierArgs, TracerEventArgs, TracingArgs},
  options::{ActivePane, Color, SeccompBpf},
};

pub mod args;
pub mod options;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
  #[arg(long, default_value_t = Color::Auto, help = "Control whether colored output is enabled")]
  pub color: Color,
  #[arg(short, long, action = ArgAction::Count)]
  pub verbose: u8,
  #[arg(short, long, conflicts_with = "verbose")]
  pub quiet: bool,
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
    tracing_args: TracingArgs,
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
      requires = "tty",
      default_value_t
    )]
    active_pane: ActivePane,
  },
}
