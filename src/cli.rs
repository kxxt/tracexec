use std::{io::stdout, path::PathBuf};

use args::{PtraceArgs, TuiModeArgs};
use clap::{CommandFactory, Parser, Subcommand};
use config::Config;
use options::ExportFormat;
use tracing::debug;

use self::{
  args::{LogModeArgs, ModifierArgs, TracerEventArgs},
  options::Color,
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
    short = 'P',
    long,
    help = "Load profile from this path",
    conflicts_with = "no_profile"
  )]
  pub profile: Option<PathBuf>,
  #[arg(long, help = "Do not load profiles")]
  pub no_profile: bool,
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
    ptrace_args: PtraceArgs,
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
    ptrace_args: PtraceArgs,
    #[clap(flatten)]
    tracer_event_args: TracerEventArgs,
    #[clap(flatten)]
    tui_args: TuiModeArgs,
  },
  #[clap(about = "Generate shell completions for tracexec")]
  GenerateCompletions {
    #[arg(required = true, help = "The shell to generate completions for")]
    shell: clap_complete::Shell,
  },
  #[clap(about = "Collect exec events and export them")]
  Collect {
    #[arg(last = true, required = true, help = "command to be executed")]
    cmd: Vec<String>,
    #[clap(flatten)]
    modifier_args: ModifierArgs,
    #[clap(flatten)]
    ptrace_args: PtraceArgs,
    #[clap(short = 'F', long, help = "the format for exported exec events")]
    format: ExportFormat,
    #[clap(short, long, help = "prettify the output if supported")]
    pretty: bool,
    #[clap(
      short,
      long,
      help = "Output, stderr by default. A single hyphen '-' represents stdout."
    )]
    output: Option<PathBuf>,
    #[clap(
      long,
      help = "Set the terminal foreground process group to tracee. This option is useful when tracexec is used interactively. [default]",
      conflicts_with = "no_foreground"
    )]
    foreground: bool,
    #[clap(
      long,
      help = "Do not set the terminal foreground process group to tracee",
      conflicts_with = "foreground"
    )]
    no_foreground: bool,
  },
  #[cfg(feature = "ebpf")]
  #[clap(about = "Experimental ebpf mode")]
  Ebpf {
    #[arg(
      last = true,
      help = "command to be executed. Leave it empty to trace all exec on system"
    )]
    cmd: Vec<String>,
    #[clap(
      short,
      long,
      help = "Output, stderr by default. A single hyphen '-' represents stdout."
    )]
    output: Option<PathBuf>,
    #[clap(flatten)]
    modifier_args: ModifierArgs,
  },
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
        ptrace_args,
        ..
      } => {
        if let Some(c) = config.ptrace {
          ptrace_args.merge_config(c);
        }
        if let Some(c) = config.modifier {
          modifier_args.merge_config(c);
        }
        if let Some(c) = config.log {
          tracing_args.merge_config(c);
        }
      }
      CliCommand::Tui {
        modifier_args,
        ptrace_args,
        tui_args,
        ..
      } => {
        if let Some(c) = config.ptrace {
          ptrace_args.merge_config(c);
        }
        if let Some(c) = config.modifier {
          modifier_args.merge_config(c);
        }
        if let Some(c) = config.tui {
          tui_args.merge_config(c);
        }
      }
      CliCommand::Collect {
        foreground,
        no_foreground,
        ptrace_args,
        ..
      } => {
        if let Some(c) = config.ptrace {
          ptrace_args.merge_config(c);
        }
        if let Some(c) = config.log {
          if (!*foreground) && (!*no_foreground) {
            if let Some(x) = c.foreground {
              if x {
                *foreground = true;
              } else {
                *no_foreground = true;
              }
            }
          }
        }
      }
      _ => (),
    }
  }
}
