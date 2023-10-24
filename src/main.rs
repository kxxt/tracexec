mod arch;
mod cli;
mod inspect;
mod printer;
mod proc;
mod state;
mod tracer;

use std::ffi::CString;

use clap::Parser;
use cli::Cli;

use crate::cli::CliCommand;

fn main() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    pretty_env_logger::init();
    log::trace!("Commandline args: {:?}", cli);
    match cli.cmd {
        CliCommand::Log { cmd, tracing_args } => {
            tracer::Tracer::new(tracing_args)
                .start_root_process(cmd.into_iter().map(|x| x).collect())?;
        }
        CliCommand::Tree { cmd, tracing_args } => {
            unimplemented!("tree mode not implemented yet")
        }
    }
    Ok(())
}
