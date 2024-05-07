use std::{
  cell::RefCell,
  collections::BTreeMap,
  ffi::OsStr,
  io::{self, Write},
  path::Path,
  sync::Arc,
};

use crate::{
  cli::args::{ModifierArgs, TracingArgs},
  event::TracerEvent,
  proc::{diff_env, BaselineInfo, FileDescriptorInfoCollection, Interpreter},
  tracer::state::ProcessState,
  tracer::InspectError,
};

use itertools::chain;
use nix::{fcntl::OFlag, libc::ENOENT, unistd::Pid};
use owo_colors::{OwoColorize, Style};

macro_rules! escape_str_for_bash {
  // TODO: This is ... quite ugly. We should find a better way to do this.
  ($x:expr) => {
    // https://github.com/rust-lang/rust/issues/64727
    String::from_utf8_lossy(&shell_quote::Bash::quote($x))
  };
}

pub(crate) use escape_str_for_bash;

#[derive(Debug, Clone, Copy)]
pub enum EnvPrintFormat {
  Diff,
  Raw,
  None,
}

#[derive(Debug, Clone, Copy)]
pub enum FdPrintFormat {
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
  pub trace_fd: FdPrintFormat,
  pub trace_cwd: bool,
  pub print_cmdline: bool,
  pub successful_only: bool,
  pub trace_interpreter: bool,
  pub trace_filename: bool,
  pub decode_errno: bool,
  pub color: ColorLevel,
  pub stdio_in_cmdline: bool,
  pub fd_in_cmdline: bool,
}

impl PrinterArgs {
  pub fn from_cli(tracing_args: &TracingArgs, modifier_args: &ModifierArgs) -> Self {
    PrinterArgs {
      trace_comm: !tracing_args.no_show_comm,
      trace_argv: !tracing_args.no_show_argv && !tracing_args.show_cmdline,
      trace_env: match (
        tracing_args.show_cmdline,
        tracing_args.diff_env,
        tracing_args.no_diff_env,
        tracing_args.show_env,
        tracing_args.no_show_env,
      ) {
        (true, ..) | (.., true) => EnvPrintFormat::None,
        (false, .., true, _) | (false, _, true, ..) => EnvPrintFormat::Raw,
        _ => EnvPrintFormat::Diff, // diff_env is enabled by default
      },
      trace_fd: match (
        tracing_args.diff_fd,
        tracing_args.no_diff_fd,
        tracing_args.show_fd,
        tracing_args.no_show_fd,
      ) {
        (false, _, true, false) => FdPrintFormat::Raw,
        (_, true, _, _) => FdPrintFormat::None,
        (true, _, _, _) => FdPrintFormat::Diff,
        _ => {
          // The default is diff fd,
          // but if fd_in_cmdline is enabled, we disable diff fd by default
          if modifier_args.fd_in_cmdline {
            FdPrintFormat::None
          } else {
            FdPrintFormat::Diff
          }
        }
      },
      trace_cwd: tracing_args.show_cwd,
      print_cmdline: tracing_args.show_cmdline,
      successful_only: modifier_args.successful_only,
      trace_interpreter: tracing_args.show_interpreter,
      trace_filename: match (
        tracing_args.show_filename,
        tracing_args.no_show_filename,
        tracing_args.show_cmdline,
      ) {
        (true, _, _) => true,
        // show filename by default, but not in show-cmdline mode
        (false, _, true) => false,
        _ => true,
      },
      decode_errno: !tracing_args.no_decode_errno,
      color: match (tracing_args.more_colors, tracing_args.less_colors) {
        (false, false) => ColorLevel::Normal,
        (true, false) => ColorLevel::More,
        (false, true) => ColorLevel::Less,
        _ => unreachable!(),
      },
      stdio_in_cmdline: modifier_args.stdio_in_cmdline,
      fd_in_cmdline: modifier_args.fd_in_cmdline,
    }
  }
}

pub type PrinterOut = dyn Write + Send + Sync + 'static;

enum DeferredWarningKind {
  NoArgv0,
  FailedReadingArgv(InspectError),
  FailedReadingFilename(InspectError),
  FailedReadingEnvp(InspectError),
}

struct DeferredWarnings {
  warning: DeferredWarningKind,
  pid: Pid,
}

impl Drop for DeferredWarnings {
  fn drop(&mut self) {
    Printer::OUT.with_borrow_mut(|out| {
      if let Some(out) = out {
        write!(out, "{}", self.pid.bright_red()).unwrap();
        write!(out, "[{}]: ", "warning".bright_yellow()).unwrap();
        match self.warning {
          DeferredWarningKind::NoArgv0 => {
            write!(
              out,
              "No argv[0] provided! The printed commandline might be incorrect!"
            )
            .unwrap();
          }
          DeferredWarningKind::FailedReadingArgv(e) => {
            write!(out, "Failed to read argv: {}", e).unwrap();
          }
          DeferredWarningKind::FailedReadingFilename(e) => {
            write!(out, "Failed to read filename: {}", e).unwrap();
          }
          DeferredWarningKind::FailedReadingEnvp(e) => {
            write!(out, "Failed to read envp: {}", e).unwrap();
          }
        };
        writeln!(out).unwrap();
      };
    })
  }
}

pub struct ListPrinter {
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

pub struct Printer {
  pub args: PrinterArgs,
  baseline: Arc<BaselineInfo>,
}

impl Printer {
  pub fn new(args: PrinterArgs, baseline: Arc<BaselineInfo>) -> Self {
    Printer { args, baseline }
  }

  thread_local! {
    pub static OUT: RefCell<Option<Box<PrinterOut>>> = RefCell::new(None);
  }

  pub fn init_thread_local(&self, output: Option<Box<PrinterOut>>) {
    Printer::OUT.with(|out| {
      *out.borrow_mut() = output;
    });
  }

  pub fn print_new_child(&self, state: &ProcessState, child: Pid) -> color_eyre::Result<()> {
    Self::OUT.with_borrow_mut(|out| {
      let Some(out) = out else {
        return Ok(());
      };
      write!(out, "{}", state.pid.bright_yellow())?;
      if self.args.trace_comm {
        write!(out, "<{}>", state.comm.cyan())?;
      }
      writeln!(out, ": {}: {}", "new child".purple(), child.bright_yellow())?;
      out.flush()?;
      Ok(())
    })
  }

  pub fn print_fd(
    &self,
    out: &mut dyn Write,
    fds: &FileDescriptorInfoCollection,
  ) -> io::Result<()> {
    match self.args.trace_fd {
      FdPrintFormat::Diff => {
        write!(out, " {} ", "fd".purple())?;
        let list_printer = ListPrinter::new(self.args.color);
        list_printer.begin(out)?;
        // Stdio
        let mut printed = 0;
        let mut last = fds.fdinfo.len();
        let fdinfo_orig = self.baseline.fdinfo.get(0).unwrap();
        if let Some(fdinfo) = fds.fdinfo.get(&0) {
          printed += 1;
          if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
            write!(out, "{}", "cloexec: stdin".bright_red().bold())?;
            if printed < last {
              list_printer.comma(out)?;
            }
          } else if fdinfo.path != fdinfo_orig.path {
            write!(out, "{}", "stdin".bright_yellow().bold())?;
            write!(out, "={}", fdinfo.path.display().bright_yellow())?;
            if printed < last {
              list_printer.comma(out)?;
            }
          }
        } else {
          printed += 1;
          write!(out, "{}", "closed: stdin".bright_red().bold())?;
          if printed < last {
            last += 1;
            list_printer.comma(out)?;
          }
        }
        let fdinfo_orig = self.baseline.fdinfo.get(1).unwrap();
        if let Some(fdinfo) = fds.fdinfo.get(&1) {
          printed += 1;
          if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
            write!(out, "{}", "cloexec: stdout".bright_red().bold())?;
            if printed < last {
              list_printer.comma(out)?;
            }
          } else if fdinfo.path != fdinfo_orig.path {
            write!(out, "{}", "stdout".bright_yellow().bold())?;
            write!(out, "={}", fdinfo.path.display().bright_yellow())?;
            if printed < last {
              list_printer.comma(out)?;
            }
          }
        } else {
          printed += 1;
          write!(out, "{}", "closed: stdout".bright_red().bold(),)?;
          if printed < last {
            last += 1;
            list_printer.comma(out)?;
          }
        }
        let fdinfo_orig = self.baseline.fdinfo.get(2).unwrap();
        if let Some(fdinfo) = fds.fdinfo.get(&2) {
          printed += 1;
          if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
            write!(out, "{}", "cloexec: stderr".bright_red().bold())?;
            if printed < last {
              list_printer.comma(out)?;
            }
          } else if fdinfo.path != fdinfo_orig.path {
            write!(out, "{}", "stderr".bright_yellow().bold())?;
            write!(out, "={}", fdinfo.path.display().bright_yellow())?;
            if printed < last {
              list_printer.comma(out)?;
            }
          }
        } else {
          printed += 1;
          write!(out, "{}", "closed: stderr".bright_red().bold(),)?;
          if printed < last {
            last += 1;
            list_printer.comma(out)?;
          }
        }
        for (&fd, fdinfo) in fds.fdinfo.iter() {
          if fd < 3 {
            continue;
          }
          printed += 1;
          if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
            write!(
              out,
              "{} {}",
              "cloexec:".bright_red().bold(),
              fd.bright_green().bold()
            )?;
            write!(out, "={}", fdinfo.path.display().bright_red())?;
          } else {
            write!(out, "{}", fd.bright_green().bold())?;
            write!(out, "={}", fdinfo.path.display().bright_green())?;
          }
          if printed < last {
            list_printer.comma(out)?;
          }
        }
        list_printer.end(out)?;
      }
      FdPrintFormat::Raw => {
        write!(out, " {} ", "fd".purple())?;
        let list_printer = ListPrinter::new(self.args.color);
        list_printer.begin(out)?;
        let last = fds.fdinfo.len() - 1;
        for (idx, (fd, fdinfo)) in fds.fdinfo.iter().enumerate() {
          write!(out, "{}", fd.bright_cyan().bold())?;
          write!(out, "={}", fdinfo.path.display())?;
          if idx != last {
            list_printer.comma(out)?;
          }
        }
        list_printer.end(out)?;
      }
      FdPrintFormat::None => {}
    }
    Ok(())
  }

  pub fn print_exec_trace(
    &self,
    state: &ProcessState,
    result: i64,
    env: &BTreeMap<String, String>,
    cwd: &Path,
  ) -> color_eyre::Result<()> {
    // Preconditions:
    // 1. execve syscall exit, which leads to 2
    // 2. state.exec_data is Some

    // Defer the warnings so that they are printed after the main message
    let mut _deferred_warnings = vec![];

    Self::OUT.with_borrow_mut(|out| {
      let Some(out) = out else {
        return Ok(());
      };
      let exec_data = state.exec_data.as_ref().unwrap();
      let list_printer = ListPrinter::new(self.args.color);
      if result == 0 {
        write!(out, "{}", state.pid.bright_green())?;
      } else if result == -ENOENT as i64 {
        write!(out, "{}", state.pid.bright_yellow())?;
      } else {
        write!(out, "{}", state.pid.bright_red())?;
      }
      if self.args.trace_comm {
        write!(out, "<{}>", state.comm.cyan())?;
      }
      write!(out, ":")?;

      match exec_data.filename.as_ref() {
        Ok(filename) => {
          if self.args.trace_filename {
            write!(out, " {:?}", filename)?;
          }
        }
        Err(e) => {
          write!(
            out,
            " {}",
            format!("[Failed to read filename: {e}]")
              .bright_red()
              .blink()
              .bold()
          )?;
          _deferred_warnings.push(DeferredWarnings {
            warning: DeferredWarningKind::FailedReadingFilename(*e),
            pid: state.pid,
          });
        }
      }

      match exec_data.argv.as_ref() {
        Err(e) => {
          _deferred_warnings.push(DeferredWarnings {
            warning: DeferredWarningKind::FailedReadingArgv(*e),
            pid: state.pid,
          });
        }
        Ok(argv) => {
          if self.args.trace_argv {
            write!(out, " ")?;
            list_printer.print_string_list(out, argv)?;
          }
        }
      }

      // CWD

      if self.args.trace_cwd {
        write!(out, " {} {:?}", "at".purple(), exec_data.cwd)?;
      }

      // Interpreter

      if self.args.trace_interpreter && result == 0 {
        // FIXME: show interpreter for errnos other than ENOENT
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

      // File descriptors

      self.print_fd(out, &exec_data.fdinfo)?;

      // Environment

      match exec_data.envp.as_ref() {
        Ok(envp) => {
          match self.args.trace_env {
            EnvPrintFormat::Diff => {
              // TODO: make it faster
              //       This is mostly a proof of concept
              write!(out, " {} ", "with".purple())?;
              list_printer.begin(out)?;
              let env = env.clone();
              let mut first_item_written = false;
              let mut write_separator = |out: &mut dyn Write| -> io::Result<()> {
                if first_item_written {
                  list_printer.comma(out)?;
                } else {
                  first_item_written = true;
                }
                Ok(())
              };

              let diff = diff_env(&env, envp);
              for (k, v) in diff.added.into_iter() {
                write_separator(out)?;
                write!(
                  out,
                  "{}{:?}{}{:?}",
                  "+".bright_green().bold(),
                  k.green(),
                  "=".bright_green().bold(),
                  v.green()
                )?;
              }
              for (k, v) in diff.modified.into_iter() {
                write_separator(out)?;
                write!(
                  out,
                  "{}{:?}{}{:?}",
                  "M".bright_yellow().bold(),
                  k.yellow(),
                  "=".bright_yellow().bold(),
                  v.bright_blue()
                )?;
              }
              // Now we have the tracee removed entries in env
              for k in diff.removed.into_iter() {
                write_separator(out)?;
                write!(
                  out,
                  "{}{:?}{}{:?}",
                  "-".bright_red().bold(),
                  k.bright_red().strikethrough(),
                  "=".bright_red().strikethrough(),
                  env.get(&k).unwrap().bright_red().strikethrough()
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
              list_printer.print_string_list(out, envp)?;
            }
            EnvPrintFormat::None => (),
          }
        }
        Err(e) => {
          match self.args.trace_env {
            EnvPrintFormat::Diff | EnvPrintFormat::Raw => {
              write!(
                out,
                " {} {}",
                "with".purple(),
                format!("[Failed to read envp: {e}]")
                  .bright_red()
                  .blink()
                  .bold()
              )?;
            }
            EnvPrintFormat::None => {}
          }
          _deferred_warnings.push(DeferredWarnings {
            warning: DeferredWarningKind::FailedReadingEnvp(*e),
            pid: state.pid,
          });
        }
      }

      // Command line

      if self.args.print_cmdline {
        write!(out, " {}", "cmdline".purple())?;
        write!(out, " env")?;

        if self.args.stdio_in_cmdline {
          let fdinfo_orig = self.baseline.fdinfo.stdin().unwrap();
          if let Some(fdinfo) = exec_data.fdinfo.stdin() {
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // stdin will be closed
              write!(out, " {}", "0>&-".bright_red().bold().italic())?;
            } else if fdinfo.path != fdinfo_orig.path {
              write!(
                out,
                " {}{}",
                "<".bright_yellow().bold(),
                escape_str_for_bash!(&fdinfo.path).bright_yellow().bold()
              )?;
            }
          } else {
            // stdin is closed
            write!(out, " {}", "0>&-".bright_red().bold())?;
          }
          let fdinfo_orig = self.baseline.fdinfo.stdout().unwrap();
          if let Some(fdinfo) = exec_data.fdinfo.stdout() {
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // stdout will be closed
              write!(out, " {}", "1>&-".bright_red().bold().italic())?;
            } else if fdinfo.path != fdinfo_orig.path {
              write!(
                out,
                " {}{}",
                ">".bright_yellow().bold(),
                escape_str_for_bash!(&fdinfo.path).bright_yellow().bold()
              )?;
            }
          } else {
            // stdout is closed
            write!(out, " {}", "1>&-".bright_red().bold())?;
          }
          let fdinfo_orig = self.baseline.fdinfo.stderr().unwrap();
          if let Some(fdinfo) = exec_data.fdinfo.stderr() {
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // stderr will be closed
              write!(out, " {}", "2>&-".bright_red().bold().italic())?;
            } else if fdinfo.path != fdinfo_orig.path {
              write!(
                out,
                " {}{}",
                "2>".bright_yellow().bold(),
                escape_str_for_bash!(&fdinfo.path).bright_yellow().bold()
              )?;
            }
          } else {
            // stderr is closed
            write!(out, " {}", "2>&-".bright_red().bold())?;
          }
        }

        if self.args.fd_in_cmdline {
          for (&fd, fdinfo) in exec_data.fdinfo.fdinfo.iter() {
            if fd < 3 {
              continue;
            }
            if fdinfo.flags.contains(OFlag::O_CLOEXEC) {
              // Don't show fds that will be closed upon exec
              continue;
            }
            write!(
              out,
              " {}{}{}",
              fd.bright_green().bold(),
              ">".bright_green().bold(),
              escape_str_for_bash!(&fdinfo.path).bright_green().bold()
            )?;
          }
        }

        match exec_data.argv.as_ref() {
          Ok(argv) => {
            if let Some(arg0) = argv.first() {
              // filename warning is already handled
              if let Ok(filename) = exec_data.filename.as_ref() {
                if filename.as_os_str() != OsStr::new(arg0) {
                  write!(
                    out,
                    " {} {}",
                    "-a".bright_white().italic(),
                    escape_str_for_bash!(arg0).bright_white().italic()
                  )?;
                }
              }
            } else {
              _deferred_warnings.push(DeferredWarnings {
                warning: DeferredWarningKind::NoArgv0,
                pid: state.pid,
              });
            }
            if cwd != exec_data.cwd {
              if self.args.color >= ColorLevel::Normal {
                write!(
                  out,
                  " -C {}",
                  escape_str_for_bash!(&exec_data.cwd).bright_cyan()
                )?;
              } else {
                write!(out, " -C {}", escape_str_for_bash!(&exec_data.cwd))?;
              }
            }
            // envp warning is already handled
            if let Ok(envp) = exec_data.envp.as_ref() {
              let diff = diff_env(env, envp);
              // Now we have the tracee removed entries in env
              for k in diff.removed.into_iter() {
                if self.args.color >= ColorLevel::Normal {
                  write!(
                    out,
                    " {}{}",
                    "-u ".bright_red(),
                    escape_str_for_bash!(&k).bright_red()
                  )?;
                } else {
                  write!(out, " -u={}", escape_str_for_bash!(&k))?;
                }
              }
              if self.args.color >= ColorLevel::Normal {
                for (k, v) in diff.added.into_iter() {
                  write!(
                    out,
                    " {}{}{}",
                    escape_str_for_bash!(&k).green(),
                    "=".green().bold(),
                    escape_str_for_bash!(&v).green()
                  )?;
                }
                for (k, v) in diff.modified.into_iter() {
                  write!(
                    out,
                    " {}{}{}",
                    escape_str_for_bash!(&k),
                    "=".bright_yellow().bold(),
                    escape_str_for_bash!(&v).bright_blue()
                  )?;
                }
              } else {
                for (k, v) in chain!(diff.added.into_iter(), diff.modified.into_iter()) {
                  write!(
                    out,
                    " {}={}",
                    escape_str_for_bash!(&k),
                    escape_str_for_bash!(&v)
                  )?;
                }
              }
            }
            write!(
              out,
              " {}",
              escape_str_for_bash!(TracerEvent::filename_to_cow(&exec_data.filename).as_ref())
            )?;
            for arg in argv.iter().skip(1) {
              write!(out, " {}", escape_str_for_bash!(arg))?;
            }
          }
          Err(e) => {
            _deferred_warnings.push(DeferredWarnings {
              warning: DeferredWarningKind::FailedReadingArgv(*e),
              pid: state.pid,
            });
          }
        }
      }

      // Result

      if result == 0 {
        writeln!(out)?;
      } else {
        write!(out, " {} ", "=".purple())?;
        if self.args.decode_errno {
          writeln!(
            out,
            "{} ({})",
            result.bright_red().bold(),
            nix::errno::Errno::from_raw(-result as i32).red()
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
    })
  }
}
