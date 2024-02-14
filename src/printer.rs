
use std::{
    collections::HashMap,
    io::{self, Write},
    path::Path,
};

use crate::{proc::Interpreter, state::ProcessState};

use nix::unistd::Pid;
use owo_colors::{OwoColorize, Style};

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
        String::from_utf8_lossy(&shell_quote::Bash::quote($x))
    };
}

#[derive(Debug, Clone, Copy)]
pub enum EnvPrintFormat {
    Diff,
    Raw,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum ColorLevel {
    Less,
    Normal,
    More,
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
    pub color: ColorLevel,
}

pub fn print_new_child(
    out: &mut dyn Write,
    state: &ProcessState,
    args: &PrinterArgs,
    child: Pid,
) -> color_eyre::Result<()> {
    write!(out, "{}", state.pid.bright_yellow())?;
    if args.trace_comm {
        write!(out, "<{}>", state.comm.cyan())?;
    }
    writeln!(out, ": {}: {}", "new child".purple(), child.bright_yellow())?;
    out.flush()?;
    Ok(())
}

pub fn print_exec_trace(
    out: &mut dyn Write,
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
    let list_printer = ListPrinter::new(args.color);
    if result == 0 {
        write!(out, "{}", state.pid.bright_yellow())?;
    } else {
        write!(out, "{}", state.pid.bright_red())?;
    }
    if args.trace_comm {
        write!(out, "<{}>", state.comm.cyan())?;
    }
    write!(out, ":")?;
    if args.trace_filename {
        write!(out, " {:?}", exec_data.filename)?;
    }
    if args.trace_argv {
        write!(out, " ")?;
        list_printer.print_string_list(out, &exec_data.argv)?;
    }
    if args.trace_cwd {
        write!(out, " {} {:?}", "at".purple(), exec_data.cwd)?;
    }
    if args.trace_interpreter && result == 0 {
        write!(out, " {} ", "interpreter".purple(),)?;
        match exec_data.interpreters.len() {
            0 => {
                write!(out, "{}", Interpreter::None)?;
            }
            1 => {
                write!(out, "{}", exec_data.interpreters[0])?;
            }
            _ => {
                list_printer.begin(out)?;
                for (idx, interpreter) in exec_data.interpreters.iter().enumerate() {
                    if idx != 0 {
                        list_printer.comma(out)?;
                    }
                    write!(out, "{}", interpreter)?;
                }
                list_printer.end(out)?;
            }
        }
    }
    match args.trace_env {
        EnvPrintFormat::Diff => {
            // TODO: make it faster
            //       This is mostly a proof of concept
            write!(out, " {} ", "with".purple())?;
            list_printer.begin(out)?;
            let mut env = env.clone();
            let mut first_item_written = false;
            let mut write_separator = |out: &mut dyn Write| -> io::Result<()> {
                if first_item_written {
                    list_printer.comma(out)?;
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
                        write_separator(out)?;
                        write!(
                            out,
                            "{}{:?}={:?}",
                            "M".bright_yellow().bold(),
                            k,
                            v.bright_blue()
                        )?;
                    }
                    // Remove existing entry
                    env.remove(k);
                } else {
                    write_separator(out)?;
                    write!(
                        out,
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
                write_separator(out)?;
                write!(
                    out,
                    "{}{:?}{}{:?}",
                    "-".bright_red().bold(),
                    k.bright_red().strikethrough(),
                    "=".bright_red().strikethrough(),
                    v.bright_red().strikethrough()
                )?;
            }
            list_printer.end(out)?;
            // Avoid trailing color
            // https://unix.stackexchange.com/questions/212933/background-color-whitespace-when-end-of-the-terminal-reached
            if owo_colors::control::should_colorize() {
                write!(out, "\x1B[49m\x1B[K")?;
            }
        }
        EnvPrintFormat::Raw => {
            write!(out, " {} ", "with".purple())?;
            list_printer.print_string_list(out, &exec_data.envp)?;
        }
        EnvPrintFormat::None => (),
    }
    let mut deferred_warning = DeferredWarnings {
        warning: DeferredWarningKind::None,
    };
    if args.print_cmdline {
        write!(out, " {}", "cmdline".purple())?;
        write!(out, " env")?;
        if cwd != exec_data.cwd {
            if args.color >= ColorLevel::Normal {
                write!(
                    out,
                    " -C {}",
                    escape_str_for_bash!(&exec_data.cwd).bright_cyan()
                )?;
            } else {
                write!(out, " -C {}", escape_str_for_bash!(&exec_data.cwd))?;
            }
        }
        let mut env = env.clone();
        let mut updated = Vec::new(); // k,v,is_new
        for item in exec_data.envp.iter() {
            let (k, v) = parse_env_entry(item);
            // Too bad that we still don't have if- and while-let-chains
            // https://github.com/rust-lang/rust/issues/53667
            if let Some(orig_v) = env.get(k).map(|x| x.as_str()) {
                if orig_v != v {
                    updated.push((k, v, false));
                }
                // Remove existing entry
                env.remove(k);
            } else {
                updated.push((k, v, true));
            }
        }
        // Now we have the tracee removed entries in env
        for (k, _v) in env.iter() {
            if args.color >= ColorLevel::Normal {
                write!(
                    out,
                    " {}{}",
                    "-u ".bright_red(),
                    escape_str_for_bash!(k).bright_red()
                )?;
            } else {
                write!(out, " -u={}", escape_str_for_bash!(k))?;
            }
        }
        if args.color >= ColorLevel::Normal {
            for (k, v, is_new) in updated.into_iter() {
                if is_new {
                    write!(
                        out,
                        " {}{}{}",
                        escape_str_for_bash!(k).green(),
                        "=".green().bold(),
                        escape_str_for_bash!(v).green()
                    )?;
                } else {
                    write!(
                        out,
                        " {}{}{}",
                        escape_str_for_bash!(k),
                        "=".bold(),
                        escape_str_for_bash!(v).bright_blue()
                    )?;
                }
            }
        } else {
            for (k, v, _) in updated.into_iter() {
                write!(
                    out,
                    " {}={}",
                    escape_str_for_bash!(k),
                    escape_str_for_bash!(v)
                )?;
            }
        }
        for (idx, arg) in exec_data.argv.iter().enumerate() {
            if idx == 0 {
                let escaped_filename = shell_quote::Bash::quote(&exec_data.filename);
                let escaped_filename_lossy = String::from_utf8_lossy(&escaped_filename);
                if !escaped_filename_lossy.ends_with(arg) {
                    deferred_warning.warning = DeferredWarningKind::Argv0AndFileNameDiffers;
                }
                write!(out, " {}", escaped_filename_lossy)?;
                continue;
            }
            write!(out, " {}", escape_str_for_bash!(arg))?;
        }
    }
    if result == 0 {
        writeln!(out)?;
    } else {
        write!(out, " {} ", "=".purple())?;
        if args.decode_errno {
            writeln!(
                out,
                "{} ({})",
                result.bright_red().bold(),
                nix::errno::Errno::from_i32(-result as i32).red()
            )?;
        } else {
            writeln!(out, "{}", result.bright_red().bold())?;
        }
    }
    // It is critical to call [flush] before BufWriter<W> is dropped.
    // Though dropping will attempt to flush the contents of the buffer, any errors that happen in the process of dropping will be ignored.
    // Calling [flush] ensures that the buffer is empty and thus dropping will not even attempt file operations.
    out.flush()?;
    Ok(())
}

enum DeferredWarningKind {
    None,
    Argv0AndFileNameDiffers,
}

struct DeferredWarnings {
    warning: DeferredWarningKind,
}

impl Drop for DeferredWarnings {
    fn drop(&mut self) {
        match self.warning {
            DeferredWarningKind::None => (),
            DeferredWarningKind::Argv0AndFileNameDiffers => log::warn!(
                "argv[0] and filename differs. The printed commandline might be incorrect!"
            ),
        }
    }
}

struct ListPrinter {
    style: owo_colors::Style,
}

impl ListPrinter {
    pub fn new(color: ColorLevel) -> Self {
        if color > ColorLevel::Normal {
            ListPrinter {
                style: Style::new().bright_white().bold(),
            }
        } else {
            ListPrinter {
                style: Style::new(),
            }
        }
    }

    pub fn begin(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(out, "{}", "[".style(self.style))
    }

    pub fn end(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(out, "{}", "]".style(self.style))
    }

    pub fn comma(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(out, "{}", ", ".style(self.style))
    }

    pub fn print_string_list(&self, out: &mut dyn Write, list: &[String]) -> io::Result<()> {
        self.begin(out)?;
        if let Some((last, rest)) = list.split_last() {
            if rest.is_empty() {
                write!(out, "{:?}", last)?;
            } else {
                for s in rest {
                    write!(out, "{:?}", s)?;
                    self.comma(out)?;
                }
                write!(out, "{:?}", last)?;
            }
        }
        self.end(out)
    }
}
