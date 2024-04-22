mod arch;
mod cli;
mod event;
mod inspect;
mod log;
mod printer;
mod proc;
#[cfg(feature = "seccomp-bpf")]
mod seccomp;
mod state;
mod tracer;
mod tui;

use std::{
    io::{stderr, stdout, BufWriter, Write},
    mem::forget,
    os::unix::ffi::OsStrExt,
    process, thread,
};

use atoi::atoi;
use clap::Parser;
use cli::Cli;
use color_eyre::eyre::bail;
use crossterm::event::KeyCode;
use ratatui::widgets::Widget;
use tokio::{sync::mpsc, task::spawn_blocking};

use crate::{
    cli::{CliCommand, Color},
    event::{Event, TracerEvent},
    log::initialize_panic_handler,
    tui::event_list::{EventList, EventListApp},
};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
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
    initialize_panic_handler();
    log::initialize_logging()?;
    // TODO: separate output verbosity from log level
    // pretty_env_logger::formatted_builder()
    //     .filter_level(match (cli.quiet, cli.verbose) {
    //         // Don't follow RUST_LOG environment variable.
    //         (true, _) => log::LevelFilter::Error,
    //         (false, 0) => log::LevelFilter::Warn,
    //         (false, 1) => log::LevelFilter::Info,
    //         (false, 2) => log::LevelFilter::Debug,
    //         (false, _) => log::LevelFilter::Trace,
    //     })
    //     .init();
    log::trace!("Commandline args: {:?}", cli);
    // Seccomp-bpf ptrace behavior is changed on 4.8. I haven't tested on older kernels.
    let min_support_kver = (4, 8);
    if !is_current_kernel_greater_than(min_support_kver)? {
        log::warn!(
            "Current kernel version is not supported! Minimum supported kernel version is {}.{}.",
            min_support_kver.0,
            min_support_kver.1
        );
    }
    match cli.cmd {
        CliCommand::Log {
            cmd,
            tracing_args,
            output,
            tui,
        } => {
            let output: Box<dyn Write + Send> = match output {
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
            let mut app = EventListApp {
                event_list: EventList::new(),
            };
            let (tracer_tx, mut tracer_rx) = mpsc::unbounded_channel();
            let mut tracer = tracer::Tracer::new(tracing_args, output, tracer_tx)?;
            let tracer_thread = thread::spawn(move || tracer.start_root_process(cmd));
            if tui {
                let mut tui = tui::Tui::new()?.frame_rate(30.0);
                tui.enter(tracer_rx)?;
                loop {
                    if let Some(e) = tui.next().await {
                        match e {
                            Event::ShouldQuit => {
                                break;
                            }
                            Event::Key(ke) => {
                                if ke.code == KeyCode::Char('q') {
                                    // todo
                                }
                            }
                            Event::Tracer(te) => {}
                            Event::Render => {
                                tui.draw(|f| app.render(f.size(), f.buffer_mut()))?;
                            }
                            Event::Init => {}
                            Event::Error => {}
                        }
                    }
                }
                tui::restore_tui()?;
            } else {
                tracer_thread.join().unwrap()?;
                loop {
                    if let Some(e) = tracer_rx.recv().await {
                        match e {
                            TracerEvent::RootChildExit { exit_code, .. } => {
                                process::exit(exit_code);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_current_kernel_greater_than(min_support: (u32, u32)) -> color_eyre::Result<bool> {
    let utsname = nix::sys::utsname::uname()?;
    let kstr = utsname.release().as_bytes();
    let pos = kstr.iter().position(|&c| c != b'.' && !c.is_ascii_digit());
    let kver = if let Some(pos) = pos {
        let (s, _) = kstr.split_at(pos);
        s
    } else {
        kstr
    };
    let mut kvers = kver.split(|&c| c == b'.');
    let Some(major) = kvers.next().and_then(atoi::<u32>) else {
        bail!("Failed to parse kernel major ver!")
    };
    let Some(minor) = kvers.next().and_then(atoi::<u32>) else {
        bail!("Failed to parse kernel minor ver!")
    };
    Ok((major, minor) >= min_support)
}
