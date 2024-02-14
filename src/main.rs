mod arch;
mod cli;
mod inspect;
mod printer;
mod proc;
mod seccomp;
mod state;
mod tracer;

use std::io::{stderr, stdout, BufWriter, Write};

use clap::Parser;
use cli::Cli;

use crate::cli::{CliCommand, Color};

fn main() -> color_eyre::Result<()> {
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
    pretty_env_logger::formatted_builder()
        .filter_level(match (cli.quiet, cli.verbose) {
            // Don't follow RUST_LOG environment variable.
            (true, _) => log::LevelFilter::Error,
            (false, 0) => log::LevelFilter::Warn,
            (false, 1) => log::LevelFilter::Info,
            (false, 2) => log::LevelFilter::Debug,
            (false, _) => log::LevelFilter::Trace,
        })
        .init();
    log::trace!("Commandline args: {:?}", cli);
    match cli.cmd {
        CliCommand::Log {
            cmd,
            tracing_args,
            output,
        } => {
            let output: Box<dyn Write> = match output {
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
            tracer::Tracer::new(tracing_args, output)?.start_root_process(cmd)?;
        }
        CliCommand::Tree {
            cmd: _,
            tracing_args: _,
        } => {
            unimplemented!("tree mode not implemented yet")
        }
    }
    Ok(())
}
