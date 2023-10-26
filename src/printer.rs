use std::{
    collections::HashMap,
    io::{self, stdout, Stdout, Write},
    path::Path,
};

use crate::{proc::Interpreter, state::ProcessState};

use owo_colors::OwoColorize;

fn parse_env_entry(item: &str) -> (&str, &str) {
    let mut sep_loc = item
        .as_bytes()
        .iter()
        .position(|&x| x == b'=')
        .unwrap_or_else(|| {
            log::warn!(
                "Invalid envp entry: {:?}, assuming value to empty string!",
                item
            );
            item.len()
        });
    if sep_loc == 0 {
        // Find the next equal sign
        sep_loc = item
            .as_bytes()
            .iter()
            .skip(1)
            .position(|&x| x == b'=')
            .unwrap_or_else(|| {
                log::warn!(
                    "Invalid envp entry staring with '=': {:?}, assuming value to empty string!",
                    item
                );
                item.len()
            });
    }
    let (head, tail) = item.split_at(sep_loc);
    (head, &tail[1..])
}

macro_rules! escape_str_for_bash {
    // TODO: This is ... quite ugly. We should find a better way to do this.
    ($x:expr) => {
        // https://github.com/rust-lang/rust/issues/64727
        String::from_utf8_lossy(&shell_quote::bash::escape($x))
    };
}

#[derive(Debug, Clone, Copy)]
pub enum EnvPrintFormat {
    Diff,
    Raw,
    None,
}

#[derive(Debug, Clone)]
pub struct PrinterArgs {
    pub trace_comm: bool,
    pub trace_argv: bool,
    pub trace_env: EnvPrintFormat,
    pub trace_cwd: bool,
    pub print_cmdline: bool,
    pub successful_only: bool,
    pub trace_interpreter: bool,
    pub trace_filename: bool,
    pub decode_errno: bool,
}

pub fn print_exec_trace(
    state: &ProcessState,
    result: i64,
    args: &PrinterArgs,
    env: &HashMap<String, String>,
    cwd: &Path,
) -> color_eyre::Result<()> {
    // Preconditions:
    // 1. execve syscall exit, which leads to 2
    // 2. state.exec_data is Some
    let exec_data = state.exec_data.as_ref().unwrap();
    let mut stdout = stdout();
    if result == 0 {
        write!(stdout, "{}", state.pid.bright_yellow())?;
    } else {
        write!(stdout, "{}", state.pid.bright_red())?;
    }
    if args.trace_comm {
        write!(stdout, "<{}>", state.comm.cyan())?;
    }
    write!(stdout, ":")?;
    if args.trace_filename {
        write!(stdout, " {:?}", exec_data.filename)?;
    }
    if args.trace_argv {
        write!(stdout, " {:?}", exec_data.argv)?;
    }
    if args.trace_cwd {
        write!(stdout, " {} {:?}", "at".purple(), exec_data.cwd)?;
    }
    if args.trace_interpreter && result == 0 {
        write!(stdout, " {} ", "interpreter".purple(),)?;
        match exec_data.interpreters.len() {
            0 => {
                write!(stdout, "{}", Interpreter::None)?;
            }
            1 => {
                write!(stdout, "{}", exec_data.interpreters[0])?;
            }
            _ => {
                write!(stdout, "[")?;
                for (idx, interpreter) in exec_data.interpreters.iter().enumerate() {
                    if idx != 0 {
                        write!(stdout, ", ")?;
                    }
                    write!(stdout, "{}", interpreter)?;
                }
                write!(stdout, "]")?;
            }
        }
    }
    match args.trace_env {
        EnvPrintFormat::Diff => {
            // TODO: make it faster
            //       This is mostly a proof of concept
            write!(stdout, " {} [", "with".purple())?;
            let mut env = env.clone();
            let mut first_item_written = false;
            let mut write_separator = |out: &mut Stdout| -> io::Result<()> {
                if first_item_written {
                    write!(out, ", ")?;
                } else {
                    first_item_written = true;
                }
                Ok(())
            };
            for item in exec_data.envp.iter() {
                let (k, v) = parse_env_entry(item);
                // Too bad that we still don't have if- and while-let-chains
                // https://github.com/rust-lang/rust/issues/53667
                if let Some(orig_v) = env.get(k).map(|x| x.as_str()) {
                    if orig_v != v {
                        write_separator(&mut stdout)?;
                        write!(
                            stdout,
                            "{}{:?}={:?}",
                            "M".bright_yellow().bold(),
                            k,
                            v.bright_blue()
                        )?;
                    }
                    // Remove existing entry
                    env.remove(k);
                } else {
                    write_separator(&mut stdout)?;
                    write!(
                        stdout,
                        "{}{:?}{}{:?}",
                        "+".bright_green().bold(),
                        k.green(),
                        "=".green(),
                        v.green()
                    )?;
                }
            }
            // Now we have the tracee removed entries in env
            for (k, v) in env.iter() {
                write_separator(&mut stdout)?;
                write!(
                    stdout,
                    "{}{:?}{}{:?}",
                    "-".bright_red().bold(),
                    k.bright_red().strikethrough(),
                    "=".bright_red().strikethrough(),
                    v.bright_red().strikethrough()
                )?;
            }
            write!(stdout, "]")?;
            // Avoid trailing color
            // https://unix.stackexchange.com/questions/212933/background-color-whitespace-when-end-of-the-terminal-reached
            if owo_colors::control::should_colorize() {
                write!(stdout, "\x1B[49m\x1B[K")?;
            }
        }
        EnvPrintFormat::Raw => {
            write!(stdout, " {} {:?}", "with".purple(), exec_data.envp)?;
        }
        EnvPrintFormat::None => (),
    }
    if args.print_cmdline {
        write!(stdout, " env ")?;
        if cwd != exec_data.cwd {
            write!(stdout, "-C {} ", escape_str_for_bash!(&exec_data.cwd))?;
        }
        let mut env = env.clone();
        let mut updated = Vec::new();
        for item in exec_data.envp.iter() {
            let (k, v) = parse_env_entry(item);
            // Too bad that we still don't have if- and while-let-chains
            // https://github.com/rust-lang/rust/issues/53667
            if let Some(orig_v) = env.get(k).map(|x| x.as_str()) {
                if orig_v != v {
                    updated.push((k, v));
                }
                // Remove existing entry
                env.remove(k);
            } else {
                updated.push((k, v));
            }
        }
        // Now we have the tracee removed entries in env
        for (k, _v) in env.iter() {
            write!(stdout, "-u={} ", escape_str_for_bash!(k))?;
        }
        for (k, v) in updated.iter() {
            write!(
                stdout,
                "{}={} ",
                escape_str_for_bash!(k),
                escape_str_for_bash!(v)
            )?;
        }
        for (idx, arg) in exec_data.argv.iter().enumerate() {
            if idx == 0 && arg == "/proc/self/exe" {
                write!(stdout, "{} ", escape_str_for_bash!(&exec_data.filename))?;
                continue;
            }
            if idx != 0 {
                write!(stdout, " ")?;
            }
            write!(stdout, "{}", escape_str_for_bash!(arg))?;
        }
    }
    if result == 0 {
        writeln!(stdout)?;
    } else {
        write!(stdout, " {} ", "=".purple())?;
        if args.decode_errno {
            writeln!(
                stdout,
                "{} ({})",
                result.bright_red().bold(),
                nix::errno::Errno::from_i32(-result as i32).red()
            )?;
        } else {
            writeln!(stdout, "{}", result.bright_red().bold())?;
        }
    }
    Ok(())
}
