use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use strum::Display;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    #[arg(long, default_value_t = Color::Auto, help = "Control whether colored output is enabled")]
    pub color: Color,
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,
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
        #[clap(
            short,
            long,
            help = "Output, stderr by default. A single hyphen '-' represents stdout."
        )]
        output: Option<PathBuf>,
    },
    #[clap(
        about = "Run tracexec in TUI mode, stdin/out/err are redirected to /dev/null by default"
    )]
    Tui {
        #[arg(last = true, required = true, help = "command to be executed")]
        cmd: Vec<String>,
        #[clap(flatten)]
        tracing_args: TracingArgs,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum Color {
    Auto,
    Always,
    Never,
}

#[cfg(feature = "seccomp-bpf")]
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum SeccompBpf {
    Auto,
    On,
    Off,
}

#[derive(Args, Debug)]
pub struct TracingArgs {
    #[clap(long, help = "Only show successful calls", default_value_t = false)]
    pub successful_only: bool,
    #[clap(
        long,
        help = "Print commandline that reproduces what was executed. Note that when filename and argv[0] differs, it probably won't give you the correct commandline for now. Implies --successful-only",
        conflicts_with_all = ["show_env", "diff_env", "show_argv"]
    )]
    pub show_cmdline: bool,
    #[clap(long, help = "Try to show script interpreter indicated by shebang")]
    pub show_interpreter: bool,
    #[clap(long, help = "More colors", conflicts_with = "less_colors")]
    pub more_colors: bool,
    #[clap(long, help = "Less colors", conflicts_with = "more_colors")]
    pub less_colors: bool,
    #[clap(long, help = "Print a message when a child is created")]
    pub show_children: bool,
    #[cfg(feature = "seccomp-bpf")]
    #[clap(long, help = "seccomp-bpf filtering option", default_value_t = SeccompBpf::Auto)]
    pub seccomp_bpf: SeccompBpf,
    // BEGIN ugly: https://github.com/clap-rs/clap/issues/815
    #[clap(
        long,
        help = "Diff environment variables with the original environment",
        conflicts_with = "no_diff_env",
        conflicts_with = "show_env",
        conflicts_with = "no_show_env"
    )]
    pub diff_env: bool,
    #[clap(
        long,
        help = "Do not diff environment variables",
        conflicts_with = "diff_env"
    )]
    pub no_diff_env: bool,
    #[clap(
        long,
        help = "Show environment variables",
        conflicts_with = "no_show_env",
        conflicts_with = "diff_env"
    )]
    pub show_env: bool,
    #[clap(
        long,
        help = "Do not show environment variables",
        conflicts_with = "show_env"
    )]
    pub no_show_env: bool,
    #[clap(long, help = "Show comm", conflicts_with = "no_show_comm")]
    pub show_comm: bool,
    #[clap(long, help = "Do not show comm", conflicts_with = "show_comm")]
    pub no_show_comm: bool,
    #[clap(long, help = "Show argv", conflicts_with = "no_show_argv")]
    pub show_argv: bool,
    #[clap(long, help = "Do not show argv", conflicts_with = "show_argv")]
    pub no_show_argv: bool,
    #[clap(
        long,
        help = "Show filename",
        default_value_t = true,
        conflicts_with = "no_show_filename"
    )]
    pub show_filename: bool,
    #[clap(long, help = "Do not show filename", conflicts_with = "show_filename")]
    pub no_show_filename: bool,
    #[clap(long, help = "Show cwd", conflicts_with = "no_show_cwd")]
    pub show_cwd: bool,
    #[clap(long, help = "Do not show cwd", conflicts_with = "show_cwd")]
    pub no_show_cwd: bool,
    #[clap(long, help = "Decode errno values", conflicts_with = "no_decode_errno")]
    pub decode_errno: bool,
    #[clap(long, conflicts_with = "decode_errno")]
    pub no_decode_errno: bool,
    // END ugly
}
