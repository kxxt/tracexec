use clap::Args;
use enumflags2::BitFlags;

use crate::event::TracerEventKind;

#[cfg(feature = "seccomp-bpf")]
use super::options::SeccompBpf;

#[derive(Args, Debug)]
pub struct ModifierArgs {
  #[cfg(feature = "seccomp-bpf")]
  #[clap(long, help = "seccomp-bpf filtering option", default_value_t = SeccompBpf::Auto)]
  pub seccomp_bpf: SeccompBpf,
  #[clap(long, help = "Only show successful calls", default_value_t = false)]
  pub successful_only: bool,
}

#[derive(Args, Debug, Default)]
pub struct TracerEventArgs {
  // TODO:
  //   1. This isn't really compatible with logging mode
  //   2. Option to exclude some events instead of including some
  //   3. Option to include all events
  #[clap(
    long,
    value_delimiter = ',',
    num_args = 1..,
    help = "Only show events in this filter",
    default_values_t = [TracerEventKind::Warning, TracerEventKind::Error, TracerEventKind::Exec, TracerEventKind::RootChildExit]
  )]
  pub filter: Vec<TracerEventKind>,
}

impl TracerEventArgs {
  pub fn filter(&self) -> BitFlags<TracerEventKind> {
    self
      .filter
      .iter()
      .fold(BitFlags::empty(), |acc, f| acc | *f)
  }
}

#[derive(Args, Debug, Default)]
pub struct TracingArgs {
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
  #[clap(long, conflicts_with = "decode_errno")]
  pub no_decode_errno: bool,
  // END ugly
}
