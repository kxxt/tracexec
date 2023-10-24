use clap::{Args, Parser, Subcommand, ValueEnum};
use strum::Display;

#[derive(Parser, Debug)]
pub struct Cli {
    #[arg(long, default_value_t = Color::Auto, help = "Control whether colored output is enabled")]
    pub color: Color,
    #[clap(subcommand)]
    pub cmd: CliCommand,
}

#[derive(Subcommand, Debug)]
pub enum CliCommand {
    #[clap(about = "Run tracexec in logging mode")]
    Log {
        #[arg(last = true, required = true)]
        cmd: Vec<String>,
        #[clap(flatten)]
        tracing_args: TracingArgs,
    },
    #[clap(about = "Run tracexec in tree visualization mode")]
    Tree {
        #[arg(last = true)]
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

#[derive(Args, Debug)]
pub struct TracingArgs {
    #[clap(long, help = "Only show successful calls", default_value_t = false)]
    pub successful_only: bool,
    #[clap(
        long,
        help = "Print commandline that reproduces what was executed. Note that when filename and argv[0] differs, it won't give you the correct commandline for now. Implies --successful-only",
        conflicts_with_all = ["trace_filename", "trace_env", "diff_env", "trace_argv"]
    )]
    pub print_cmdline: bool,
    // BEGIN ugly: https://github.com/clap-rs/clap/issues/815
    #[clap(
        long,
        help = "Diff environment variables with the original environment",
        conflicts_with = "no_diff_env",
        conflicts_with = "trace_env"
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
        help = "Trace environment variables",
        conflicts_with = "no_trace_env"
    )]
    pub trace_env: bool,
    #[clap(
        long,
        help = "Do not trace environment variables",
        conflicts_with = "trace_env"
    )]
    pub no_trace_env: bool,
    #[clap(long, help = "Trace comm", conflicts_with = "no_trace_comm")]
    pub trace_comm: bool,
    #[clap(long, help = "Do not trace comm", conflicts_with = "trace_comm")]
    pub no_trace_comm: bool,
    #[clap(long, help = "Trace argv", conflicts_with = "no_trace_argv")]
    pub trace_argv: bool,
    #[clap(long, help = "Do not trace argv", conflicts_with = "trace_argv")]
    pub no_trace_argv: bool,
    #[clap(
        long,
        help = "Trace filename",
        default_value_t = true,
        conflicts_with = "no_trace_filename"
    )]
    pub trace_filename: bool,
    #[clap(
        long,
        help = "Do not trace filename",
        conflicts_with = "trace_filename"
    )]
    pub no_trace_filename: bool,
    #[clap(long, help = "Trace cwd", conflicts_with = "no_trace_cwd")]
    pub trace_cwd: bool,
    #[clap(long, help = "Do not trace cwd", conflicts_with = "trace_cwd")]
    pub no_trace_cwd: bool,
    #[clap(long, help = "Decode errno values", conflicts_with = "no_decode_errno")]
    pub decode_errno: bool,
    #[clap(long, conflicts_with = "decode_errno")]
    pub no_decode_errno: bool,
    // END ugly
}
