use std::{
  io::{
    BufWriter,
    stderr,
    stdout,
  },
  path::PathBuf,
};

use args::{
  DebuggerArgs,
  PtraceArgs,
  TuiModeArgs,
};
use clap::{
  CommandFactory,
  Parser,
  Subcommand,
};
use config::Config;
use options::ExportFormat;
use tracing::debug;

use self::{
  args::{
    LogModeArgs,
    ModifierArgs,
    TracerEventArgs,
  },
  options::Color,
};
use crate::{
  cli::args::ExporterArgs,
  output::Output,
};

pub mod args;
pub mod config;
pub mod options;
pub mod theme;

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
    #[clap(flatten)]
    debugger_args: DebuggerArgs,
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
    #[clap(flatten)]
    exporter_args: ExporterArgs,
    #[clap(short = 'F', long, help = "the format for exported exec events")]
    format: ExportFormat,
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
    #[clap(subcommand)]
    command: EbpfCommand,
  },
}

#[derive(Subcommand, Debug)]
#[cfg(feature = "ebpf")]
pub enum EbpfCommand {
  #[clap(about = "Run tracexec in logging mode")]
  Log {
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
    #[clap(flatten)]
    log_args: LogModeArgs,
  },
  #[clap(about = "Run tracexec in TUI mode, stdin/out/err are redirected to /dev/null by default")]
  Tui {
    #[arg(
      last = true,
      help = "command to be executed. Leave it empty to trace all exec on system"
    )]
    cmd: Vec<String>,
    #[clap(flatten)]
    modifier_args: ModifierArgs,
    #[clap(flatten)]
    tracer_event_args: TracerEventArgs,
    #[clap(flatten)]
    tui_args: TuiModeArgs,
  },
  #[clap(about = "Collect exec events and export them")]
  Collect {
    #[arg(
      last = true,
      help = "command to be executed. Leave it empty to trace all exec on system"
    )]
    cmd: Vec<String>,
    #[clap(flatten)]
    modifier_args: ModifierArgs,
    #[clap(short = 'F', long, help = "the format for exported exec events")]
    format: ExportFormat,
    #[clap(flatten)]
    exporter_args: ExporterArgs,
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
}

impl Cli {
  pub fn get_output(path: Option<PathBuf>, color: Color) -> std::io::Result<Box<Output>> {
    Ok(match path {
      None => Box::new(stderr()),
      Some(ref x) if x.as_os_str() == "-" => Box::new(stdout()),
      Some(path) => {
        let file = std::fs::OpenOptions::new()
          .create(true)
          .truncate(true)
          .write(true)
          .open(path)?;
        if color != Color::Always {
          // Disable color by default when output is file
          owo_colors::control::set_should_colorize(false);
        }
        Box::new(BufWriter::new(file))
      }
    })
  }

  pub fn generate_completions(shell: clap_complete::Shell) {
    let mut cmd = Self::command();
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
        debugger_args,
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
        if let Some(c) = config.debugger {
          debugger_args.merge_config(c);
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
        if let Some(c) = config.log
          && (!*foreground)
          && (!*no_foreground)
          && let Some(x) = c.foreground
        {
          if x {
            *foreground = true;
          } else {
            *no_foreground = true;
          }
        }
      }
      _ => (),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fs,
    io::Write,
    path::PathBuf,
  };

  use super::*;
  use crate::cli::{
    args::{
      DebuggerArgs,
      LogModeArgs,
      ModifierArgs,
      PtraceArgs,
      TracerEventArgs,
      TuiModeArgs,
    },
    config::{
      Config,
      DebuggerConfig,
      LogModeConfig,
      ModifierConfig,
      PtraceConfig,
      TuiModeConfig,
    },
    options::{
      Color,
      ExportFormat,
      SeccompBpf,
    },
  };

  #[test]
  fn test_cli_parse_log() {
    let args = vec![
      "tracexec",
      "log",
      "--show-interpreter",
      "--successful-only",
      "--",
      "echo",
      "hello",
    ];
    let cli = Cli::parse_from(args);

    if let CliCommand::Log {
      cmd,
      tracing_args,
      modifier_args,
      ..
    } = cli.cmd
    {
      assert_eq!(cmd, vec!["echo", "hello"]);
      assert!(tracing_args.show_interpreter);
      assert!(modifier_args.successful_only);
    } else {
      panic!("Expected Log command");
    }
  }

  #[test]
  fn test_cli_parse_tui() {
    let args = vec!["tracexec", "tui", "--tty", "--follow", "--", "bash"];
    let cli = Cli::parse_from(args);

    if let CliCommand::Tui { cmd, tui_args, .. } = cli.cmd {
      assert_eq!(cmd, vec!["bash"]);
      assert!(tui_args.tty);
      assert!(tui_args.follow);
    } else {
      panic!("Expected Tui command");
    }
  }

  #[test]
  fn test_get_output_stderr_stdout_file() {
    // default (None) -> stderr
    let out = Cli::get_output(None, Color::Auto).unwrap();
    let _ = out; // just ensure it returns something

    // "-" -> stdout
    let out = Cli::get_output(Some(PathBuf::from("-")), Color::Auto).unwrap();
    let _ = out;

    // real file
    let path = PathBuf::from("test_output.txt");
    let mut out = Cli::get_output(Some(path.clone()), Color::Auto).unwrap();
    writeln!(out, "Hello world").unwrap();
    drop(out);

    let content = fs::read_to_string(path.clone()).unwrap();
    assert!(content.contains("Hello world"));
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_merge_config_log() {
    let mut cli = Cli {
      color: Color::Auto,
      cwd: None,
      profile: None,
      no_profile: false,
      user: None,
      cmd: CliCommand::Log {
        cmd: vec!["ls".into()],
        tracing_args: LogModeArgs {
          show_interpreter: false,
          ..Default::default()
        },
        modifier_args: ModifierArgs::default(),
        ptrace_args: PtraceArgs::default(),
        tracer_event_args: TracerEventArgs::all(),
        output: None,
      },
    };

    let config = Config {
      ptrace: Some(PtraceConfig {
        seccomp_bpf: Some(SeccompBpf::On),
      }),
      modifier: Some(ModifierConfig {
        successful_only: Some(true),
        ..Default::default()
      }),
      log: Some(LogModeConfig {
        show_interpreter: Some(true),
        ..Default::default()
      }),
      tui: None,
      debugger: None,
    };

    cli.merge_config(config);

    if let CliCommand::Log {
      tracing_args,
      modifier_args,
      ptrace_args,
      ..
    } = cli.cmd
    {
      assert!(tracing_args.show_interpreter);
      assert!(modifier_args.successful_only);
      assert_eq!(ptrace_args.seccomp_bpf, SeccompBpf::On);
    } else {
      panic!("Expected Log command");
    }
  }

  #[test]
  fn test_merge_config_tui() {
    let mut cli = Cli {
      color: Color::Auto,
      cwd: None,
      profile: None,
      no_profile: false,
      user: None,
      cmd: CliCommand::Tui {
        cmd: vec!["bash".into()],
        modifier_args: ModifierArgs::default(),
        ptrace_args: PtraceArgs::default(),
        tracer_event_args: TracerEventArgs::all(),
        tui_args: TuiModeArgs::default(),
        debugger_args: DebuggerArgs::default(),
      },
    };

    let config = Config {
      ptrace: Some(PtraceConfig {
        seccomp_bpf: Some(SeccompBpf::Off),
      }),
      modifier: Some(ModifierConfig {
        successful_only: Some(true),
        ..Default::default()
      }),
      log: None,
      tui: Some(TuiModeConfig {
        follow: Some(true),
        frame_rate: Some(30.0),
        ..Default::default()
      }),
      debugger: Some(DebuggerConfig {
        default_external_command: Some("echo hello".into()),
      }),
    };

    cli.merge_config(config);

    if let CliCommand::Tui {
      modifier_args,
      ptrace_args,
      tui_args,
      debugger_args,
      ..
    } = cli.cmd
    {
      assert!(modifier_args.successful_only);
      assert_eq!(ptrace_args.seccomp_bpf, SeccompBpf::Off);
      assert_eq!(tui_args.frame_rate.unwrap(), 30.0);
      assert_eq!(
        debugger_args.default_external_command.as_ref().unwrap(),
        "echo hello"
      );
    } else {
      panic!("Expected Tui command");
    }
  }

  #[test]
  fn test_merge_config_collect_foreground() {
    let mut cli = Cli {
      color: Color::Auto,
      cwd: None,
      profile: None,
      no_profile: false,
      user: None,
      cmd: CliCommand::Collect {
        cmd: vec!["ls".into()],
        modifier_args: ModifierArgs::default(),
        ptrace_args: PtraceArgs::default(),
        exporter_args: Default::default(),
        format: ExportFormat::Json,
        output: None,
        foreground: false,
        no_foreground: false,
      },
    };

    let config = Config {
      log: Some(LogModeConfig {
        foreground: Some(true),
        ..Default::default()
      }),
      ptrace: None,
      modifier: None,
      tui: None,
      debugger: None,
    };

    cli.merge_config(config);

    if let CliCommand::Collect {
      foreground,
      no_foreground,
      ..
    } = cli.cmd
    {
      assert!(foreground);
      assert!(!no_foreground);
    } else {
      panic!("Expected Collect command");
    }
  }

  #[test]
  fn test_generate_completions_smoke() {
    // smoke test: just run without panicking
    Cli::generate_completions(clap_complete::Shell::Bash);
  }
}
