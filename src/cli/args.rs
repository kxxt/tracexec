use std::{borrow::Cow, num::ParseFloatError};

use clap::{Args, ValueEnum};
use color_eyre::eyre::bail;
use enumflags2::BitFlags;

use crate::{
  cli::config::{ColorLevel, EnvDisplay, FileDescriptorDisplay},
  event::TracerEventDetailsKind,
  tracer::state::BreakPoint,
  tui::app::AppLayout,
};

#[cfg(feature = "seccomp-bpf")]
use super::options::SeccompBpf;
use super::{
  config::{ExitHandling, LogModeConfig, ModifierConfig, TuiModeConfig},
  options::ActivePane,
};

#[derive(Args, Debug, Default, Clone)]
pub struct ModifierArgs {
  #[cfg(feature = "seccomp-bpf")]
  #[clap(long, help = "Controls whether to enable seccomp-bpf optimization, which greatly improves performance", default_value_t = SeccompBpf::Auto)]
  pub seccomp_bpf: SeccompBpf,
  #[clap(long, help = "Only show successful calls", default_value_t = false)]
  pub successful_only: bool,
  #[clap(
    long,
    help = "[Experimental] Try to reproduce file descriptors in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.",
    default_value_t = false
  )]
  pub fd_in_cmdline: bool,
  #[clap(
    long,
    help = "[Experimental] Try to reproduce stdio in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.",
    default_value_t = false
  )]
  pub stdio_in_cmdline: bool,
  #[clap(long, help = "Resolve /proc/self/exe symlink", default_value_t = false)]
  pub resolve_proc_self_exe: bool,
  #[clap(
    long,
    help = "Do not resolve /proc/self/exe symlink",
    default_value_t = false,
    conflicts_with = "resolve_proc_self_exe"
  )]
  pub no_resolve_proc_self_exe: bool,
  #[clap(
    long,
    help = "Delay between polling, in microseconds. The default is 500 when seccomp-bpf is enabled, otherwise 1."
  )]
  pub tracer_delay: Option<u64>,
}

impl ModifierArgs {
  pub fn processed(mut self) -> Self {
    self.stdio_in_cmdline = self.fd_in_cmdline || self.stdio_in_cmdline;
    self.resolve_proc_self_exe = match (self.resolve_proc_self_exe, self.no_resolve_proc_self_exe) {
      (true, false) => true,
      (false, true) => false,
      _ => true, // default
    };
    self
  }

  pub fn merge_config(&mut self, config: ModifierConfig) {
    // seccomp-bpf
    #[cfg(feature = "seccomp-bpf")]
    if let Some(setting) = config.seccomp_bpf {
      if self.seccomp_bpf == SeccompBpf::Auto {
        self.seccomp_bpf = setting;
      }
    }
    // false by default flags
    self.successful_only = self.successful_only || config.successful_only.unwrap_or_default();
    self.fd_in_cmdline |= config.fd_in_cmdline.unwrap_or_default();
    self.stdio_in_cmdline |= config.stdio_in_cmdline.unwrap_or_default();
    // flags that have negation counterparts
    if (!self.no_resolve_proc_self_exe) && (!self.resolve_proc_self_exe) {
      self.resolve_proc_self_exe = config.resolve_proc_self_exe.unwrap_or_default();
    }
  }
}

#[derive(Args, Debug, Default)]
pub struct TracerEventArgs {
  // TODO:
  //   This isn't really compatible with logging mode
  #[clap(
    long,
    help = "Set the default filter to show all events. This option can be used in combination with --filter-exclude to exclude some unwanted events.",
    conflicts_with = "filter"
  )]
  pub show_all_events: bool,
  #[clap(
    long,
    help = "Set the default filter for events.",
    value_parser = tracer_event_filter_parser,
    default_value = "warning,error,exec,tracee-exit"
  )]
  pub filter: BitFlags<TracerEventDetailsKind>,
  #[clap(
    long,
    help = "Aside from the default filter, also include the events specified here.",
    required = false,
    value_parser = tracer_event_filter_parser,
    default_value_t = BitFlags::empty()
  )]
  pub filter_include: BitFlags<TracerEventDetailsKind>,
  #[clap(
    long,
    help = "Exclude the events specified here from the default filter.",
    value_parser = tracer_event_filter_parser,
    default_value_t = BitFlags::empty()
  )]
  pub filter_exclude: BitFlags<TracerEventDetailsKind>,
}

fn tracer_event_filter_parser(filter: &str) -> Result<BitFlags<TracerEventDetailsKind>, String> {
  let mut result = BitFlags::empty();
  if filter == "<empty>" {
    return Ok(result);
  }
  for f in filter.split(',') {
    let kind = TracerEventDetailsKind::from_str(f, false)?;
    if result.contains(kind) {
      return Err(format!(
        "Event kind '{}' is already included in the filter",
        kind
      ));
    }
    result |= kind;
  }
  Ok(result)
}

impl TracerEventArgs {
  pub fn filter(&self) -> color_eyre::Result<BitFlags<TracerEventDetailsKind>> {
    let default_filter = if self.show_all_events {
      BitFlags::all()
    } else {
      self.filter
    };
    if self.filter_include.intersects(self.filter_exclude) {
      bail!("filter_include and filter_exclude cannot contain common events");
    }
    let mut filter = default_filter | self.filter_include;
    filter.remove(self.filter_exclude);
    Ok(filter)
  }
}

#[derive(Args, Debug, Default, Clone)]
pub struct LogModeArgs {
  #[clap(
    long,
    help = "Print commandline that (hopefully) reproduces what was executed. Note: file descriptors are not handled for now.",
    conflicts_with_all = ["show_env", "diff_env", "show_argv"]
)]
  pub show_cmdline: bool,
  #[clap(long, help = "More colors", conflicts_with = "less_colors")]
  pub more_colors: bool,
  #[clap(long, help = "Less colors", conflicts_with = "more_colors")]
  pub less_colors: bool,
  // BEGIN ugly: https://github.com/clap-rs/clap/issues/815
  #[clap(
    long,
    help = "Try to show script interpreter indicated by shebang",
    conflicts_with = "no_show_interpreter"
  )]
  pub show_interpreter: bool,
  #[clap(
    long,
    help = "Do not show script interpreter indicated by shebang",
    conflicts_with = "show_interpreter"
  )]
  pub no_show_interpreter: bool,
  #[clap(
    long,
    help = "Set the terminal foreground process group to tracee. This option is useful when tracexec is used interactively.",
    conflicts_with = "no_foreground"
  )]
  pub foreground: bool,
  #[clap(
    long,
    help = "Do not set the terminal foreground process group to tracee",
    conflicts_with = "foreground"
  )]
  pub no_foreground: bool,
  #[clap(
    long,
    help = "Diff file descriptors with the original std{in/out/err}",
    conflicts_with = "no_diff_fd"
  )]
  pub diff_fd: bool,
  #[clap(
    long,
    help = "Do not diff file descriptors",
    conflicts_with = "diff_fd"
  )]
  pub no_diff_fd: bool,
  #[clap(long, help = "Show file descriptors", conflicts_with = "diff_fd")]
  pub show_fd: bool,
  #[clap(
    long,
    help = "Do not show file descriptors",
    conflicts_with = "show_fd"
  )]
  pub no_show_fd: bool,
  #[clap(
    long,
    help = "Diff environment variables with the original environment",
    conflicts_with = "no_diff_env",
    conflicts_with = "show_env",
    conflicts_with = "no_show_env"
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
    help = "Show environment variables",
    conflicts_with = "no_show_env",
    conflicts_with = "diff_env"
  )]
  pub show_env: bool,
  #[clap(
    long,
    help = "Do not show environment variables",
    conflicts_with = "show_env"
  )]
  pub no_show_env: bool,
  #[clap(long, help = "Show comm", conflicts_with = "no_show_comm")]
  pub show_comm: bool,
  #[clap(long, help = "Do not show comm", conflicts_with = "show_comm")]
  pub no_show_comm: bool,
  #[clap(long, help = "Show argv", conflicts_with = "no_show_argv")]
  pub show_argv: bool,
  #[clap(long, help = "Do not show argv", conflicts_with = "show_argv")]
  pub no_show_argv: bool,
  #[clap(
    long,
    help = "Show filename",
    default_value_t = true,
    conflicts_with = "no_show_filename"
  )]
  pub show_filename: bool,
  #[clap(long, help = "Do not show filename", conflicts_with = "show_filename")]
  pub no_show_filename: bool,
  #[clap(long, help = "Show cwd", conflicts_with = "no_show_cwd")]
  pub show_cwd: bool,
  #[clap(long, help = "Do not show cwd", conflicts_with = "show_cwd")]
  pub no_show_cwd: bool,
  #[clap(long, help = "Decode errno values", conflicts_with = "no_decode_errno")]
  pub decode_errno: bool,
  #[clap(
    long,
    help = "Do not decode errno values",
    conflicts_with = "decode_errno"
  )]
  pub no_decode_errno: bool,
  // END ugly
}

impl LogModeArgs {
  pub fn merge_config(&mut self, config: LogModeConfig) {
    /// fallback to config value if both --x and --no-x are not set
    macro_rules! fallback {
      ($x:ident) => {
        ::paste::paste! {
          if (!self.$x) && (!self.[<no_ $x>]) {
            if let Some(x) = config.$x {
              if x {
                self.$x = true;
              } else {
                self.[<no_ $x>] = true;
              }
            }
          }
        }
      };
    }
    fallback!(show_interpreter);
    fallback!(foreground);
    fallback!(show_comm);
    fallback!(show_argv);
    fallback!(show_filename);
    fallback!(show_cwd);
    fallback!(decode_errno);
    match config.fd_display {
      Some(FileDescriptorDisplay::Show) => {
        if (!self.no_show_fd) && (!self.diff_fd) {
          self.show_fd = true;
        }
      }
      Some(FileDescriptorDisplay::Diff) => {
        if (!self.show_fd) && (!self.no_diff_fd) {
          self.diff_fd = true;
        }
      }
      Some(FileDescriptorDisplay::Hide) => {
        if (!self.diff_fd) && (!self.show_fd) {
          self.no_diff_fd = true;
          self.no_show_fd = true;
        }
      }
      _ => (),
    }
    match config.env_display {
      Some(EnvDisplay::Show) => {
        if (!self.diff_env) && (!self.no_show_env) {
          self.show_env = true;
        }
      }
      Some(EnvDisplay::Diff) => {
        if (!self.show_env) && (!self.no_diff_env) {
          self.diff_env = true;
        }
      }
      Some(EnvDisplay::Hide) => {
        if (!self.show_env) && (!self.diff_env) {
          self.no_diff_env = true;
          self.no_show_env = true;
        }
      }
      _ => (),
    }
    match config.color_level {
      Some(ColorLevel::Less) => {
        if !self.more_colors {
          self.less_colors = true;
        }
      }
      Some(ColorLevel::More) => {
        if !self.less_colors {
          self.more_colors = true;
        }
      }
      _ => (),
    }
  }
}

#[derive(Args, Debug, Default, Clone)]
pub struct TuiModeArgs {
  #[clap(
    long,
    short,
    help = "Allocate a pseudo terminal and show it alongside the TUI"
  )]
  pub tty: bool,
  #[clap(long, short, help = "Keep the event list scrolled to the bottom")]
  pub follow: bool,
  #[clap(
    long,
    help = "Instead of waiting for the root child to exit, terminate when the TUI exits",
    conflicts_with = "kill_on_exit"
  )]
  pub terminate_on_exit: bool,
  #[clap(
    long,
    help = "Instead of waiting for the root child to exit, kill when the TUI exits"
  )]
  pub kill_on_exit: bool,
  #[clap(
    long,
    short = 'A',
    help = "Set the default active pane to use when TUI launches",
    requires = "tty"
  )]
  pub active_pane: Option<ActivePane>,
  #[clap(
    long,
    short = 'L',
    help = "Set the layout of the TUI when it launches",
    requires = "tty"
  )]
  pub layout: Option<AppLayout>,
  #[clap(
    long,
    short = 'F',
    help = "Set the frame rate of the TUI (60 by default)",
    value_parser = frame_rate_parser
  )]
  pub frame_rate: Option<f64>,
  #[clap(
    long,
    short = 'D',
    help = "Set the default external command to run when using \"Detach, Stop and Run Command\" feature in Hit Manager"
  )]
  pub default_external_command: Option<String>,
  #[clap(
    long = "add-breakpoint",
    short = 'b',
    value_parser = breakpoint_parser,
    help = "Add a new breakpoint to the tracer. This option can be used multiple times. The format is <syscall-stop>:<pattern-type>:<pattern>, where syscall-stop can be sysenter or sysexit, pattern-type can be argv-regex, in-filename or exact-filename. For example, sysexit:in-filename:/bash",
  )]
  pub breakpoints: Vec<BreakPoint>,
}

impl TuiModeArgs {
  pub fn merge_config(&mut self, config: TuiModeConfig) {
    self.active_pane = self.active_pane.or(config.active_pane);
    self.layout = self.layout.or(config.layout);
    if self.default_external_command.is_none() {
      self.default_external_command = config.default_external_command;
    }
    self.frame_rate = self.frame_rate.or(config.frame_rate);
    self.follow |= config.follow.unwrap_or_default();
    if (!self.terminate_on_exit) && (!self.kill_on_exit) {
      match config.exit_handling {
        Some(ExitHandling::Kill) => self.kill_on_exit = true,
        Some(ExitHandling::Terminate) => self.terminate_on_exit = true,
        _ => (),
      }
    }
  }
}

fn frame_rate_parser(s: &str) -> Result<f64, ParseFrameRateError> {
  let v = s.parse::<f64>()?;
  if v < 0.0 || v.is_nan() || v.is_infinite() {
    Err(ParseFrameRateError::InvalidFrameRate)
  } else if v < 5.0 {
    Err(ParseFrameRateError::FrameRateTooLow)
  } else {
    Ok(v)
  }
}

fn breakpoint_parser(s: &str) -> Result<BreakPoint, Cow<'static, str>> {
  BreakPoint::try_from(s)
}

#[derive(thiserror::Error, Debug)]
enum ParseFrameRateError {
  #[error("Failed to parse frame rate {0} as a floating point number")]
  ParseFloatError(ParseFloatError),
  #[error("Invalid frame rate")]
  InvalidFrameRate,
  #[error("Frame rate too low, must be at least 5.0")]
  FrameRateTooLow,
}

impl From<ParseFloatError> for ParseFrameRateError {
  fn from(e: ParseFloatError) -> Self {
    Self::ParseFloatError(e)
  }
}
