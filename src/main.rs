mod arch;
mod cli;
mod inspect;
mod printer;
mod proc;
mod state;
mod tracer;

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
    pretty_env_logger::init();
    log::trace!("Commandline args: {:?}", cli);
    match cli.cmd {
        CliCommand::Log { cmd, tracing_args } => {
            tracer::Tracer::new(tracing_args, cli.color)?.start_root_process(cmd)?;
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
