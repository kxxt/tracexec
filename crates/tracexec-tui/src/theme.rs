use std::{
  fs,
  path::{
    Path,
    PathBuf,
  },
  sync::{
    LazyLock,
    OnceLock,
  },
};

use color_eyre::eyre::{
  WrapErr,
  bail,
};
use ratatui::{
  style::{
    Color,
    Modifier,
    Style,
    Stylize,
  },
  text::Span,
};
use tracexec_core::cli::config::project_directory;
pub use tracexec_core::cli::tui_theme::{
  SpanSpec,
  StyleSpec,
  ThemeColor,
  ThemeFile,
  ThemeModifier,
  ThemeSpec,
};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct Theme {
  pub inactive_border: Style,
  pub active_border: Style,
  #[allow(unused)]
  pub popup_border: Style,
  pub app_title: Style,
  pub help_popup: Style,
  pub inline_timestamp: Style,
  pub cli_flag: Style,
  pub help_key: Style,
  pub help_desc: Style,
  pub fancy_help_desc: Style,
  pub pid_success: Style,
  pub pid_failure: Style,
  pub pid_enoent: Style,
  pub pid_in_msg: Style,
  pub comm: Style,
  pub tracer_info: Style,
  pub tracer_warning: Style,
  pub tracer_error: Style,
  pub new_child_pid: Style,
  pub tracer_event: Style,
  pub inline_tracer_error: Style,
  pub filename: Style,
  pub modified_fd_in_cmdline: Style,
  pub removed_fd_in_cmdline: Style,
  pub cloexec_fd_in_cmdline: Style,
  pub added_fd_in_cmdline: Style,
  pub arg0: Style,
  pub cwd: Style,
  pub deleted_env_var: Style,
  pub modified_env_var: Style,
  pub added_env_var: Style,
  pub argv: Style,
  pub search_match: Style,
  pub query_no_match: Style,
  pub query_match_current_no: Style,
  pub query_match_total_cnt: Style,
  pub empty_field: Style,
  pub uid_gid_name: Style,
  pub uid_gid_value: Style,
  pub exec_result_success: Style,
  pub exec_result_failure: Style,
  pub value_unknown: Style,
  pub fd_closed: Style,
  pub plus_sign: Style,
  pub minus_sign: Style,
  pub equal_sign: Style,
  pub added_env_key: Style,
  pub added_env_val: Style,
  pub removed_env_key: Style,
  pub removed_env_val: Style,
  pub unchanged_env_key: Style,
  pub unchanged_env_val: Style,
  pub fd_label: Style,
  pub fd_number_label: Style,
  pub sublabel: Style,
  pub selected_label: Style,
  pub label: Style,
  pub selection_indicator: Style,
  pub open_flag_cloexec: Style,
  pub open_flag_access_mode: Style,
  pub open_flag_creation: Style,
  pub open_flag_status: Style,
  pub open_flag_other: Style,
  pub visual_separator: Style,
  pub error_popup: Style,
  pub info_popup: Style,
  pub active_tab: Style,
  pub status_process_running: Style,
  pub status_process_paused: Style,
  pub status_process_detached: Style,
  pub status_exec_error: Style,
  pub status_process_exited_normally: Style,
  pub status_process_exited_abnormally: Style,
  pub status_process_killed: Style,
  pub status_process_terminated: Style,
  pub status_process_interrupted: Style,
  pub status_process_segfault: Style,
  pub status_process_aborted: Style,
  pub status_process_sigill: Style,
  pub status_process_signaled: Style,
  pub status_internal_failure: Style,
  pub breakpoint_title_selected: Style,
  pub breakpoint_title: Style,
  pub breakpoint_pattern_type_label: Style,
  pub breakpoint_pattern: Style,
  pub breakpoint_info_label: Style,
  pub breakpoint_info_label_active: Style,
  pub breakpoint_info_value: Style,
  pub hit_entry_pid: Style,
  pub hit_entry_plain_text: Style,
  pub hit_entry_breakpoint_stop: Style,
  pub hit_entry_breakpoint_pattern: Style,
  pub hit_entry_no_breakpoint_pattern: Style,
  pub hit_manager_default_command: Style,
  pub hit_manager_no_default_command: Style,
  pub backtrace_parent_spawns: Span<'static>,
  pub backtrace_parent_becomes: Span<'static>,
  pub backtrace_parent_unknown: Span<'static>,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      inactive_border: Style::default().white(),
      active_border: Style::default().cyan(),
      popup_border: Style::default(),
      app_title: Style::default().bold(),
      help_popup: Style::default().black().on_gray(),
      inline_timestamp: Style::default().light_cyan(),
      cli_flag: Style::default().yellow().on_dark_gray().bold(),
      help_key: Style::default().black().on_cyan().bold(),
      help_desc: Style::default()
        .light_green()
        .on_dark_gray()
        .italic()
        .bold(),
      fancy_help_desc: Style::default().red().on_light_yellow().bold().slow_blink(),
      pid_success: Style::default().light_green(),
      pid_failure: Style::default().light_red(),
      pid_enoent: Style::default().light_yellow(),
      pid_in_msg: Style::default().light_magenta(),
      comm: Style::default().cyan(),
      tracer_info: Style::default().light_blue().bold(),
      tracer_warning: Style::default().light_yellow().bold(),
      tracer_error: Style::default().light_red().bold(),
      new_child_pid: Style::default().yellow(),
      tracer_event: Style::default().magenta(),
      inline_tracer_error: Style::default().light_red().bold().slow_blink(),
      filename: Style::default().light_blue(),
      modified_fd_in_cmdline: Style::default().light_yellow().bold(),
      removed_fd_in_cmdline: Style::default().light_red().bold(),
      added_fd_in_cmdline: Style::default().light_green().bold(),
      cloexec_fd_in_cmdline: Style::default().light_red().bold().italic(),
      arg0: Style::default().white().italic(),
      cwd: Style::default().light_cyan(),
      deleted_env_var: Style::default().light_red(),
      modified_env_var: Style::default().yellow(),
      added_env_var: Style::default().green(),
      argv: Style::default(),
      search_match: Style::default().add_modifier(Modifier::REVERSED),
      query_no_match: Style::default().light_red(),
      query_match_current_no: Style::default().light_cyan(),
      query_match_total_cnt: Style::default().white(),
      empty_field: Style::default().bold(),
      uid_gid_name: Style::default().white().bold(),
      uid_gid_value: Style::default().italic(),
      exec_result_success: Style::default().green(),
      exec_result_failure: Style::default().red(),
      fd_closed: Style::default().light_red(),
      value_unknown: Style::default().light_red().italic(),
      plus_sign: Style::default().light_green(),
      minus_sign: Style::default().light_red(),
      equal_sign: Style::default().yellow().bold(),
      added_env_key: Style::default().light_green().bold(),
      added_env_val: Style::default().light_green(),
      removed_env_key: Style::default().light_red().bold(),
      removed_env_val: Style::default().light_red(),
      unchanged_env_key: Style::default().white().bold(),
      unchanged_env_val: Style::default().white(),
      fd_label: Style::default().black().on_light_green().bold(),
      fd_number_label: Style::default().white().on_light_magenta().bold(),
      sublabel: Style::default().white().bold(),
      label: Style::default().black().on_light_green().bold(),
      selected_label: Style::default().white().on_light_magenta().bold(),
      selection_indicator: Style::default().light_green().bold(),
      open_flag_cloexec: Style::default().light_green().bold(),
      open_flag_access_mode: Style::default().light_blue().bold(),
      open_flag_creation: Style::default().light_cyan().bold(),
      open_flag_status: Style::default().light_yellow().bold(),
      open_flag_other: Style::default().light_red().bold(),
      visual_separator: Style::default().light_green(),
      error_popup: Style::default().white().on_red(),
      info_popup: Style::default().black().on_white(),
      active_tab: Style::default().white().on_magenta(),
      status_process_running: Style::new().light_green().bold(),
      status_process_paused: Style::new().yellow().bold(),
      status_process_detached: Style::new().light_magenta().bold(),
      status_exec_error: Style::new().light_red().bold(),
      status_process_exited_normally: Style::new().green().bold(),
      status_process_exited_abnormally: Style::new().light_yellow().bold(),
      status_process_killed: Style::new().light_red().bold().italic(),
      status_process_terminated: Style::new().light_red().bold().italic(),
      status_process_interrupted: Style::new().light_red().bold().italic(),
      status_process_segfault: Style::new().light_red().bold().italic(),
      status_process_aborted: Style::new().light_red().bold().italic(),
      status_process_sigill: Style::new().light_red().bold().italic(),
      status_process_signaled: Style::new().light_red().bold().italic(),
      status_internal_failure: Style::new().light_red().bold().italic(),
      breakpoint_title_selected: Style::default().white().bold().on_magenta(),
      breakpoint_title: Style::default().white().bold(),
      breakpoint_pattern_type_label: Style::default().black().on_light_green(),
      breakpoint_pattern: Style::default().cyan().bold(),
      breakpoint_info_label: Style::default().black().on_light_yellow(),
      breakpoint_info_label_active: Style::default().black().on_light_green(),
      breakpoint_info_value: Style::default().black().bold().on_light_cyan(),
      hit_entry_pid: Style::default().light_magenta(),
      hit_entry_plain_text: Style::default().white().bold(),
      hit_entry_breakpoint_stop: Style::default().yellow().bold(),
      hit_entry_breakpoint_pattern: Style::default().cyan().bold(),
      hit_entry_no_breakpoint_pattern: Style::default().light_red().bold(),
      hit_manager_default_command: Style::default().light_cyan().bold(),
      hit_manager_no_default_command: Style::default().light_yellow().bold(),
      backtrace_parent_spawns: Span::raw(" S ").on_gray().light_blue().bold(),
      backtrace_parent_becomes: Span::raw(" B ").on_white().light_red().bold(),
      backtrace_parent_unknown: Span::raw("   "),
    }
  }
}

/// Parse a TOML theme file string, warn about unknown sections/fields, and return
/// the `ThemeSpec` extracted from the `[tui]` section.
///
/// The file must contain a `[tui]` section; missing it is an error.  Unknown
/// top-level sections and unknown style keys both produce warnings.
pub fn theme_spec_from_toml_str(toml: &str, source: &str) -> color_eyre::Result<ThemeSpec> {
  let file: ThemeFile =
    toml::from_str(toml).wrap_err_with(|| format!("Failed to parse theme file from {source}"))?;
  file.warn_unknown_sections(source);
  let spec = file.tui.ok_or_else(|| {
    color_eyre::eyre::eyre!("Theme file {source} is missing required [tui] section")
  })?;
  spec.warn_unknown_fields(source, Some("tui"));
  Ok(spec)
}

/// Apply a `ThemeSpec` on top of a base `Theme`, converting TOML spec values to
/// ratatui style types.
fn apply_theme_spec(spec: ThemeSpec, mut theme: Theme) -> color_eyre::Result<Theme> {
  macro_rules! apply_style_fields {
    ($($field:ident),* $(,)?) => {
      $(
        if let Some(spec) = spec.$field {
          theme.$field = apply_style_spec(spec, theme.$field)?;
        }
      )*
    };
  }

  apply_style_fields!(
    inactive_border,
    active_border,
    popup_border,
    app_title,
    help_popup,
    inline_timestamp,
    cli_flag,
    help_key,
    help_desc,
    fancy_help_desc,
    pid_success,
    pid_failure,
    pid_enoent,
    pid_in_msg,
    comm,
    tracer_info,
    tracer_warning,
    tracer_error,
    new_child_pid,
    tracer_event,
    inline_tracer_error,
    filename,
    modified_fd_in_cmdline,
    removed_fd_in_cmdline,
    cloexec_fd_in_cmdline,
    added_fd_in_cmdline,
    arg0,
    cwd,
    deleted_env_var,
    modified_env_var,
    added_env_var,
    argv,
    search_match,
    query_no_match,
    query_match_current_no,
    query_match_total_cnt,
    empty_field,
    uid_gid_name,
    uid_gid_value,
    exec_result_success,
    exec_result_failure,
    value_unknown,
    fd_closed,
    plus_sign,
    minus_sign,
    equal_sign,
    added_env_key,
    added_env_val,
    removed_env_key,
    removed_env_val,
    unchanged_env_key,
    unchanged_env_val,
    fd_label,
    fd_number_label,
    sublabel,
    selected_label,
    label,
    selection_indicator,
    open_flag_cloexec,
    open_flag_access_mode,
    open_flag_creation,
    open_flag_status,
    open_flag_other,
    visual_separator,
    error_popup,
    info_popup,
    active_tab,
    status_process_running,
    status_process_paused,
    status_process_detached,
    status_exec_error,
    status_process_exited_normally,
    status_process_exited_abnormally,
    status_process_killed,
    status_process_terminated,
    status_process_interrupted,
    status_process_segfault,
    status_process_aborted,
    status_process_sigill,
    status_process_signaled,
    status_internal_failure,
    breakpoint_title_selected,
    breakpoint_title,
    breakpoint_pattern_type_label,
    breakpoint_pattern,
    breakpoint_info_label,
    breakpoint_info_label_active,
    breakpoint_info_value,
    hit_entry_pid,
    hit_entry_plain_text,
    hit_entry_breakpoint_stop,
    hit_entry_breakpoint_pattern,
    hit_entry_no_breakpoint_pattern,
    hit_manager_default_command,
    hit_manager_no_default_command
  );

  if let Some(s) = spec.backtrace_parent_spawns {
    theme.backtrace_parent_spawns = apply_span_spec(s, &theme.backtrace_parent_spawns)?;
  }
  if let Some(s) = spec.backtrace_parent_becomes {
    theme.backtrace_parent_becomes = apply_span_spec(s, &theme.backtrace_parent_becomes)?;
  }
  if let Some(s) = spec.backtrace_parent_unknown {
    theme.backtrace_parent_unknown = apply_span_spec(s, &theme.backtrace_parent_unknown)?;
  }

  Ok(theme)
}

fn apply_style_spec(spec: StyleSpec, base: Style) -> color_eyre::Result<Style> {
  let mut patch = Style::default();
  if let Some(fg) = spec.fg {
    patch = patch.fg(theme_color_into_ratatui(fg)?);
  }
  if let Some(bg) = spec.bg {
    patch = patch.bg(theme_color_into_ratatui(bg)?);
  }
  if let Some(uc) = spec.underline_color {
    patch.underline_color = Some(theme_color_into_ratatui(uc)?);
  }
  let modifiers = modifier_bits(&spec.modifiers);
  if !modifiers.is_empty() {
    patch = patch.add_modifier(modifiers);
  }
  let removed = modifier_bits(&spec.remove_modifiers);
  if !removed.is_empty() {
    patch = patch.remove_modifier(removed);
  }
  Ok(base.patch(patch))
}

fn apply_span_spec(spec: SpanSpec, base: &Span<'static>) -> color_eyre::Result<Span<'static>> {
  let style = apply_style_spec(spec.style, base.style)?;
  let content = spec.content.unwrap_or_else(|| base.content.to_string());
  Ok(Span::styled(content, style))
}

fn theme_color_into_ratatui(color: ThemeColor) -> color_eyre::Result<Color> {
  match color {
    ThemeColor::Named(name) => parse_named_color(&name),
    ThemeColor::Indexed(index) => Ok(Color::Indexed(index)),
    ThemeColor::Rgb { r, g, b } => Ok(Color::Rgb(r, g, b)),
  }
}

fn modifier_bits(modifiers: &[ThemeModifier]) -> Modifier {
  modifiers.iter().fold(Modifier::empty(), |acc, modifier| {
    acc
      | match modifier {
        ThemeModifier::Bold => Modifier::BOLD,
        ThemeModifier::Dim => Modifier::DIM,
        ThemeModifier::Italic => Modifier::ITALIC,
        ThemeModifier::Underlined => Modifier::UNDERLINED,
        ThemeModifier::SlowBlink => Modifier::SLOW_BLINK,
        ThemeModifier::RapidBlink => Modifier::RAPID_BLINK,
        ThemeModifier::Reversed => Modifier::REVERSED,
        ThemeModifier::Hidden => Modifier::HIDDEN,
        ThemeModifier::CrossedOut => Modifier::CROSSED_OUT,
      }
  })
}

fn parse_named_color(name: &str) -> color_eyre::Result<Color> {
  let normalized = name.trim().to_ascii_lowercase();
  if let Some(hex) = normalized.strip_prefix('#') {
    if hex.len() != 6 {
      bail!("Invalid hex color '{name}': expected #RRGGBB");
    }
    let r =
      u8::from_str_radix(&hex[0..2], 16).wrap_err_with(|| format!("Invalid hex color '{name}'"))?;
    let g =
      u8::from_str_radix(&hex[2..4], 16).wrap_err_with(|| format!("Invalid hex color '{name}'"))?;
    let b =
      u8::from_str_radix(&hex[4..6], 16).wrap_err_with(|| format!("Invalid hex color '{name}'"))?;
    return Ok(Color::Rgb(r, g, b));
  }

  Ok(match normalized.as_str() {
    "reset" => Color::Reset,
    "black" => Color::Black,
    "red" => Color::Red,
    "green" => Color::Green,
    "yellow" => Color::Yellow,
    "blue" => Color::Blue,
    "magenta" => Color::Magenta,
    "cyan" => Color::Cyan,
    "gray" | "grey" => Color::Gray,
    "dark-gray" | "dark-grey" => Color::DarkGray,
    "light-red" => Color::LightRed,
    "light-green" => Color::LightGreen,
    "light-yellow" => Color::LightYellow,
    "light-blue" => Color::LightBlue,
    "light-magenta" => Color::LightMagenta,
    "light-cyan" => Color::LightCyan,
    "white" => Color::White,
    other => bail!("Unsupported color '{other}'"),
  })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThemeDirectories {
  /// `$XDG_CONFIG_HOME/tracexec/themes`
  user_config_dir: Option<PathBuf>,
  /// `$XDG_DATA_HOME/tracexec/themes`.
  user_dir: Option<PathBuf>,
  etc_dir: PathBuf,
  system_dir: PathBuf,
}

impl ThemeDirectories {
  fn for_executable(executable_path: &Path) -> Self {
    let (user_config_dir, user_dir) = project_directory()
      .map(|dirs| {
        (
          Some(dirs.config_dir().join("themes")),
          Some(dirs.data_dir().join("themes")),
        )
      })
      .unwrap_or((None, None));
    let etc_dir = PathBuf::from("/etc/tracexec/themes");
    let system_dir = executable_path.parent().map_or_else(
      || PathBuf::from("/usr/share/tracexec/themes"),
      |parent| {
        parent
          .join("..")
          .join("share")
          .join("tracexec")
          .join("themes")
      },
    );
    Self {
      user_config_dir,
      user_dir,
      etc_dir,
      system_dir,
    }
  }

  /// Resolve a (possibly relative) theme path.
  ///
  /// `cwd` is passed when the path came from the CLI so that a relative path is
  /// tried relative to the working directory before falling back to theme directories.
  #[allow(unused)]
  fn resolve(&self, requested: &Path, cwd: Option<&Path>) -> PathBuf {
    if requested.is_absolute() {
      return requested.to_path_buf();
    }
    // When coming from the CLI, try CWD first.
    if let Some(cwd) = cwd {
      let cwd_candidate = cwd.join(requested);
      if cwd_candidate.is_file() {
        return cwd_candidate;
      }
    }
    if let Some(user_config_dir) = &self.user_config_dir {
      let candidate = user_config_dir.join(requested);
      if candidate.is_file() {
        return candidate;
      }
    }
    if let Some(user_dir) = &self.user_dir {
      let user_candidate = user_dir.join(requested);
      if user_candidate.is_file() {
        return user_candidate;
      }
    }
    let etc_candidate = self.etc_dir.join(requested);
    if etc_candidate.is_file() {
      return etc_candidate;
    }
    self.system_dir.join(requested)
  }

  /// Returns all candidate paths for a relative theme name, in resolution order.
  ///
  /// Used to generate a helpful error message when none of the candidates exist.
  fn relative_candidates(&self, requested: &Path, cwd: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(cwd) = cwd {
      candidates.push(cwd.join(requested));
    }
    if let Some(d) = &self.user_config_dir {
      candidates.push(d.join(requested));
    }
    if let Some(d) = &self.user_dir {
      candidates.push(d.join(requested));
    }
    candidates.push(self.etc_dir.join(requested));
    candidates.push(self.system_dir.join(requested));
    candidates
  }
}

static DEFAULT_THEME: LazyLock<Theme> = LazyLock::new(Theme::default);
static ACTIVE_THEME: OnceLock<Theme> = OnceLock::new();

pub fn current_theme() -> &'static Theme {
  ACTIVE_THEME.get().unwrap_or(&DEFAULT_THEME)
}

pub fn initialize(
  theme_file: Option<&Path>,
  inline_theme: Option<&ThemeSpec>,
  executable_path: &Path,
  from_cli: bool,
) -> color_eyre::Result<()> {
  // If the theme file came from the CLI, ignore the inline theme config
  let inline_theme = if theme_file.is_some() && from_cli {
    None
  } else {
    inline_theme
  };

  if theme_file.is_some() && inline_theme.is_some() {
    bail!("TUI theme configuration cannot specify both 'theme-file' and 'theme'");
  }

  let loaded = match (theme_file, inline_theme) {
    (Some(path), None) => Some(load_theme_from_file(path, executable_path, from_cli)?),
    (None, Some(spec)) => Some(load_theme_from_spec(spec)?),
    (None, None) => None,
    (Some(_), Some(_)) => unreachable!(),
  };

  if let Some(theme) = loaded {
    ACTIVE_THEME
      .set(theme)
      .map_err(|_| color_eyre::eyre::eyre!("Failed to set active theme"))?;
  }
  Ok(())
}

/// Load a theme from a file on disk.
///
/// `from_cli` controls whether relative paths are first tried relative to the CWD.
pub fn load_theme_from_file(
  requested_path: &Path,
  executable_path: &Path,
  from_cli: bool,
) -> color_eyre::Result<Theme> {
  let directories = ThemeDirectories::for_executable(executable_path);
  let cwd = if from_cli {
    std::env::current_dir()
      .inspect_err(|e| {
        warn!(
          "Failed to get current directory: {}. Not using CWD for theme-file resolution.",
          e
        )
      })
      .ok()
  } else {
    None
  };
  let resolved_path = if requested_path.is_absolute() {
    requested_path.to_path_buf()
  } else {
    let candidates = directories.relative_candidates(requested_path, cwd.as_deref());
    match candidates.iter().find(|p| p.is_file()) {
      Some(p) => p.clone(),
      None => {
        let list = candidates
          .iter()
          .map(|p| format!("  - {}", p.display()))
          .collect::<Vec<_>>()
          .join("\n");
        bail!(
          "Theme file '{}' not found. Searched in order:\n{}",
          requested_path.display(),
          list
        );
      }
    }
  };
  let theme_toml = fs::read_to_string(&resolved_path)
    .wrap_err_with(|| format!("Failed to read theme file {}", resolved_path.display()))?;
  let source = resolved_path.display().to_string();
  theme_spec_from_toml_str(&theme_toml, &source)
    .and_then(|spec| apply_theme_spec(spec, Theme::default()))
    .wrap_err_with(|| format!("Failed to load theme from {source}"))
}

/// Apply a `ThemeSpec` on top of the default theme and return the result.
pub fn load_theme_from_spec(spec: &ThemeSpec) -> color_eyre::Result<Theme> {
  spec.warn_unknown_fields("inline theme config", None);
  apply_theme_spec(spec.clone(), Theme::default())
}

#[cfg(test)]
mod tests {
  use std::{
    env,
    fs,
    path::PathBuf,
    time::{
      SystemTime,
      UNIX_EPOCH,
    },
  };

  use ratatui::style::{
    Color,
    Modifier,
  };

  use super::*;

  fn temp_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    env::temp_dir().join(format!(
      "tracexec-theme-{label}-{unique}-{}",
      std::process::id()
    ))
  }

  #[test]
  fn load_theme_from_inline_spec_preserves_unspecified_style_bits() {
    let spec: ThemeSpec = toml::from_str("app-title = { fg = 'cyan' }").unwrap();
    let loaded = load_theme_from_spec(&spec).unwrap();

    assert_eq!(loaded.app_title.fg, Some(Color::Cyan));
    assert!(loaded.app_title.add_modifier.contains(Modifier::BOLD));
  }

  #[test]
  fn theme_directories_prefer_user_theme_directory() {
    let root = temp_path("user-preferred");
    let user_dir = root.join("user");
    let system_dir = root.join("system");
    let directories = ThemeDirectories {
      user_config_dir: None,
      user_dir: Some(user_dir.clone()),
      etc_dir: root.join("etc"),
      system_dir: system_dir.clone(),
    };
    fs::create_dir_all(&user_dir).unwrap();
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(user_dir.join("nord.toml"), "").unwrap();
    fs::write(system_dir.join("nord.toml"), "").unwrap();

    assert_eq!(
      directories.resolve(Path::new("nord.toml"), None),
      user_dir.join("nord.toml")
    );

    let _ = fs::remove_dir_all(root);
  }

  #[test]
  fn theme_directories_prefer_config_dir_over_data_dir() {
    let root = temp_path("config-over-data");
    let config_dir = root.join("config");
    let user_dir = root.join("data");
    let system_dir = root.join("system");
    let directories = ThemeDirectories {
      user_config_dir: Some(config_dir.clone()),
      user_dir: Some(user_dir.clone()),
      etc_dir: root.join("etc"),
      system_dir: system_dir.clone(),
    };
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(&user_dir).unwrap();
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(config_dir.join("nord.toml"), "").unwrap();
    fs::write(user_dir.join("nord.toml"), "").unwrap();
    fs::write(system_dir.join("nord.toml"), "").unwrap();

    assert_eq!(
      directories.resolve(Path::new("nord.toml"), None),
      config_dir.join("nord.toml")
    );

    let _ = fs::remove_dir_all(root);
  }

  #[test]
  fn theme_directories_prefer_etc_over_system_directory() {
    let root = temp_path("etc-preferred");
    let etc_dir = root.join("etc");
    let system_dir = root.join("system");
    let directories = ThemeDirectories {
      user_config_dir: None,
      user_dir: Some(root.join("user")),
      etc_dir: etc_dir.clone(),
      system_dir: system_dir.clone(),
    };
    fs::create_dir_all(&etc_dir).unwrap();
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(etc_dir.join("amber.toml"), "").unwrap();
    fs::write(system_dir.join("amber.toml"), "").unwrap();

    assert_eq!(
      directories.resolve(Path::new("amber.toml"), None),
      etc_dir.join("amber.toml")
    );

    let _ = fs::remove_dir_all(root);
  }

  #[test]
  fn theme_directories_fall_back_to_system_directory() {
    let root = temp_path("system-fallback");
    let system_dir = root.join("system");
    let directories = ThemeDirectories {
      user_config_dir: None,
      user_dir: Some(root.join("user")),
      etc_dir: root.join("etc"),
      system_dir: system_dir.clone(),
    };
    fs::create_dir_all(&system_dir).unwrap();

    assert_eq!(
      directories.resolve(Path::new("amber.toml"), None),
      system_dir.join("amber.toml")
    );

    let _ = fs::remove_dir_all(root);
  }

  #[test]
  fn theme_directories_cwd_takes_precedence_when_from_cli() {
    let root = temp_path("cwd-resolution");
    let cwd = root.join("cwd");
    let system_dir = root.join("system");
    let directories = ThemeDirectories {
      user_config_dir: None,
      user_dir: None,
      etc_dir: root.join("etc"),
      system_dir: system_dir.clone(),
    };
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(cwd.join("my.toml"), "").unwrap();
    fs::write(system_dir.join("my.toml"), "").unwrap();

    assert_eq!(
      directories.resolve(Path::new("my.toml"), Some(&cwd)),
      cwd.join("my.toml")
    );

    let _ = fs::remove_dir_all(root);
  }

  #[test]
  fn initialize_rejects_multiple_theme_sources() {
    let inline_theme = ThemeSpec::default();
    let err = initialize(
      Some(Path::new("theme.toml")),
      Some(&inline_theme),
      Path::new("/usr/bin/tracexec"),
      false,
    )
    .unwrap_err();

    assert!(
      err
        .to_string()
        .contains("cannot specify both 'theme-file' and 'theme'")
    );
  }

  #[test]
  fn load_theme_file_from_absolute_path() {
    let root = temp_path("absolute-path");
    fs::create_dir_all(&root).unwrap();
    let theme_path = root.join("theme.toml");
    fs::write(&theme_path, "[tui]\napp-title = { fg = 'cyan' }").unwrap();

    let theme = load_theme_from_file(&theme_path, Path::new("/usr/bin/tracexec"), false).unwrap();
    assert_eq!(theme.app_title.fg, Some(Color::Cyan));

    let _ = fs::remove_dir_all(root);
  }
}
