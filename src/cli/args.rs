use clap::{Args, ValueEnum};
use color_eyre::eyre::bail;
use enumflags2::BitFlags;

use crate::event::TracerEventKind;

#[cfg(feature = "seccomp-bpf")]
use super::options::SeccompBpf;

#[derive(Args, Debug, Default, Clone)]
pub struct ModifierArgs {
  #[cfg(feature = "seccomp-bpf")]
  #[clap(long, help = "seccomp-bpf filtering option", default_value_t = SeccompBpf::Auto)]
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
  pub filter: BitFlags<TracerEventKind>,
  #[clap(
    long,
    help = "Aside from the default filter, also include the events specified here.",
    required = false,
    value_parser = tracer_event_filter_parser,
    default_value_t = BitFlags::empty()
  )]
  pub filter_include: BitFlags<TracerEventKind>,
  #[clap(
    long,
    help = "Exclude the events specified here from the default filter.",
    value_parser = tracer_event_filter_parser,
    default_value_t = BitFlags::empty()
  )]
  pub filter_exclude: BitFlags<TracerEventKind>,
}

fn tracer_event_filter_parser(filter: &str) -> Result<BitFlags<TracerEventKind>, String> {
  let mut result = BitFlags::empty();
  if filter == "<empty>" {
    return Ok(result);
  }
  for f in filter.split(',') {
    let kind = TracerEventKind::from_str(f, false)?;
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
  pub fn filter(&self) -> color_eyre::Result<BitFlags<TracerEventKind>> {
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

#[derive(Args, Debug, Default)]
pub struct LogModeArgs {
  #[clap(
    long,
    help = "Print commandline that (hopefully) reproduces what was executed. Note: file descriptors are not handled for now.",
    conflicts_with_all = ["show_env", "diff_env", "show_argv"]
)]
  pub show_cmdline: bool,
  #[clap(long, help = "Try to show script interpreter indicated by shebang")]
  pub show_interpreter: bool,
  #[clap(long, help = "More colors", conflicts_with = "less_colors")]
  pub more_colors: bool,
  #[clap(long, help = "Less colors", conflicts_with = "more_colors")]
  pub less_colors: bool,
  // BEGIN ugly: https://github.com/clap-rs/clap/issues/815
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
