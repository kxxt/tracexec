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
    #[clap(about = "logging mode")]
    Log {
        #[arg(last = true)]
        cmd: Vec<String>,
        #[clap(flatten)]
        tracing_args: TracingArgs,
        #[clap(long, help = "Indent output", default_value_t = 0)]
        indent: u8,
    },
    #[clap(about = "tree visualization mode")]
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
    #[clap(long, help = "Diff environment variables, implies --trace-env")]
    pub diff_env: bool,
    #[clap(long, help = "Trace environment variables")]
    pub trace_env: bool,
    #[clap(long, help = "Trace argv")]
    pub trace_argv: bool,
    #[clap(long, help = "Trace filename")]
    pub trace_filename: bool,
    #[clap(long, help = "Only show successful calls")]
    pub successful_only: bool,
    #[clap(long, help = "Decode errno values")]
    pub decode_errno: bool,
}
