use std::{
    collections::HashMap,
    io::{stdout, Write},
    path::Path,
};

use crate::{
    cli::{Color, TracingArgs},
    proc::Interpreter,
    state::ProcessState,
};

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
        shell_quote::bash::quote($x).as_os_str().to_str().unwrap()
    };
}

pub fn print_execve_trace(
    state: &ProcessState,
    result: i64,
    tracing_args: &TracingArgs,
    env: &HashMap<String, String>,
    cwd: &Path,
    color: Color,
) -> color_eyre::Result<()> {
    // Preconditions:
    // 1. execve syscall exit, which leads to 2
    // 2. state.exec_data is Some
    let exec_data = state.exec_data.as_ref().unwrap();
    let mut stdout = stdout();
    // TODO: move these calculations elsewhere
    let trace_comm = !tracing_args.no_trace_comm;
    let trace_argv = !tracing_args.no_trace_argv && !tracing_args.print_cmdline;
    let trace_env = tracing_args.trace_env && !tracing_args.print_cmdline;
    let diff_env = !tracing_args.no_diff_env && !trace_env && !tracing_args.print_cmdline;
    let trace_filename = !tracing_args.no_trace_filename && !tracing_args.print_cmdline;
    let successful_only = tracing_args.successful_only || tracing_args.print_cmdline;
    let trace_cwd = tracing_args.trace_cwd && !tracing_args.print_cmdline;
    if successful_only && result != 0 {
        return Ok(());
    }
    if result == 0 {
        write!(stdout, "{}", state.pid.bright_yellow())?;
    } else {
        write!(stdout, "{}", state.pid.bright_red())?;
    }
    if trace_comm {
        write!(stdout, "<{}>", state.comm.cyan())?;
    }
    write!(stdout, ":")?;
    if trace_filename {
        write!(stdout, " {:?}", exec_data.filename)?;
    }
    if trace_argv {
        write!(stdout, " {:?}", exec_data.argv)?;
    }
    if trace_cwd {
        write!(stdout, " {} {:?}", "at".purple(), exec_data.cwd)?;
    }
    if tracing_args.trace_interpreter && result == 0 {
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
    if diff_env {
        // TODO: make it faster
        //       This is mostly a proof of concept
        write!(stdout, " {} [", "with".purple())?;
        let mut env = env.clone();
        for item in exec_data.envp.iter() {
            let (k, v) = parse_env_entry(item);
            // Too bad that we still don't have if- and while-let-chains
            // https://github.com/rust-lang/rust/issues/53667
            if let Some(orig_v) = env.get(k).map(|x| x.as_str()) {
                if orig_v != v {
                    write!(
                        stdout,
                        "{}{:?}={:?}, ",
                        "M".bright_yellow().bold(),
                        k,
                        v.on_blue()
                    )?;
                }
                // Remove existing entry
                env.remove(k);
            } else {
                write!(
                    stdout,
                    "{}{:?}{}{:?}, ",
                    "+".bright_green().bold(),
                    k.on_green(),
                    "=".on_green(),
                    v.on_green()
                )?;
            }
        }
        // Now we have the tracee removed entries in env
        for (k, v) in env.iter() {
            write!(
                stdout,
                "{}{:?}{}{:?}, ",
                "-".bright_red().bold(),
                k.on_red().strikethrough(),
                "=".on_red().strikethrough(),
                v.on_red().strikethrough()
            )?;
        }
        write!(stdout, "]")?;
        // Avoid trailing color
        // https://unix.stackexchange.com/questions/212933/background-color-whitespace-when-end-of-the-terminal-reached
        if owo_colors::control::should_colorize() {
            write!(stdout, "\x1B[49m\x1B[K")?;
        }
    } else if trace_env {
        write!(stdout, " {} {:?}", "with".purple(), exec_data.envp)?;
    } else if tracing_args.print_cmdline {
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
        let decode_errno = !tracing_args.no_decode_errno;
        write!(stdout, " {} ", "=".purple())?;
        if decode_errno {
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
