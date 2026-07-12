use std::{
  io::Write,
  path::Path,
};

use tokio::sync::mpsc::UnboundedReceiver;
use tracexec_core::{
  cli::{
    args::{
      LogModeArgs,
      TuiModeArgs,
    },
    options::ExportFormat,
  },
  event::TracerMessage,
  export::{
    Exporter,
    ExporterMetadata,
  },
  proc::BaselineInfo,
  pty::{
    PtySize,
    PtySystem,
    UnixMasterPty,
    native_pty_system,
  },
  tracer::TracerMode,
};
use tracexec_exporter_json::{
  JsonExporter,
  JsonStreamExporter,
};
use tracexec_exporter_perfetto::PerfettoExporter;

pub fn initialize_tui(tui_args: &TuiModeArgs, executable_path: &Path) -> color_eyre::Result<()> {
  tracexec_tui::theme::initialize(
    tui_args.theme_file.as_ref().map(|path| path.as_deref()),
    tui_args.theme.as_deref(),
    executable_path,
    tui_args
      .theme_file
      .as_ref()
      .is_some_and(|path| path.is_from_cli()),
  )?;

  // TUI colors are provided by ratatui rather than owo-colors.
  owo_colors::control::set_should_colorize(false);
  Ok(())
}

pub fn setup_tui_io(
  tty: bool,
  tracee_env: Option<&[(std::ffi::OsString, std::ffi::OsString)]>,
) -> color_eyre::Result<(BaselineInfo, TracerMode, Option<UnixMasterPty>)> {
  if !tty {
    return Ok((
      BaselineInfo::new_with_env(tracee_env)?,
      TracerMode::Tui(None),
      None,
    ));
  }

  let pair = native_pty_system().openpty(PtySize::default())?;
  Ok((
    BaselineInfo::with_pts_and_env(&pair.slave, tracee_env)?,
    TracerMode::Tui(Some(pair.slave)),
    Some(pair.master),
  ))
}

pub fn tui_log_args() -> LogModeArgs {
  LogModeArgs {
    show_cmdline: false, // We handle cmdline in TUI
    show_argv: true,
    show_interpreter: true,
    more_colors: false,
    less_colors: false,
    diff_env: true,
    ..Default::default()
  }
}

pub fn collect_log_args(foreground: bool, no_foreground: bool) -> LogModeArgs {
  LogModeArgs {
    show_cmdline: false,
    show_argv: true,
    show_interpreter: true,
    more_colors: false,
    less_colors: false,
    diff_env: false,
    foreground,
    no_foreground,
    ..Default::default()
  }
}

pub async fn run_exporter(
  format: ExportFormat,
  output: Box<dyn Write + Send + Sync + 'static>,
  metadata: ExporterMetadata,
  events: UnboundedReceiver<TracerMessage>,
) -> color_eyre::Result<i32> {
  match format {
    ExportFormat::Json => JsonExporter::new(output, metadata, events)?.run().await,
    ExportFormat::JsonStream => {
      JsonStreamExporter::new(output, metadata, events)?
        .run()
        .await
    }
    ExportFormat::Perfetto => PerfettoExporter::new(output, metadata, events)?.run().await,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tui_log_args_enable_tui_rendering_inputs() {
    let args = tui_log_args();

    assert!(!args.show_cmdline);
    assert!(args.show_argv);
    assert!(args.show_interpreter);
    assert!(args.diff_env);
    assert!(!args.more_colors);
    assert!(!args.less_colors);
  }

  #[test]
  fn collect_log_args_preserve_foreground_choice() {
    let args = collect_log_args(true, false);

    assert!(args.show_argv);
    assert!(args.show_interpreter);
    assert!(!args.diff_env);
    assert!(args.foreground);
    assert!(!args.no_foreground);
  }
}
