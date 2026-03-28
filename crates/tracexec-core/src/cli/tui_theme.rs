use std::collections::HashMap;

use serde::{
  Deserialize,
  Serialize,
};

/// Partial theme specification loaded from a TOML file or inline config table.
/// Only fields that are explicitly set override the defaults; all others fall
/// through to the built-in theme.
///
/// Unknown keys are **not** rejected — they are collected in `extra` so callers
/// can emit user-visible warnings at an appropriate point.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ThemeSpec {
  pub inactive_border: Option<StyleSpec>,
  pub active_border: Option<StyleSpec>,
  pub popup_border: Option<StyleSpec>,
  pub app_title: Option<StyleSpec>,
  pub help_popup: Option<StyleSpec>,
  pub inline_timestamp: Option<StyleSpec>,
  pub cli_flag: Option<StyleSpec>,
  pub help_key: Option<StyleSpec>,
  pub help_desc: Option<StyleSpec>,
  pub fancy_help_desc: Option<StyleSpec>,
  pub pid_success: Option<StyleSpec>,
  pub pid_failure: Option<StyleSpec>,
  pub pid_enoent: Option<StyleSpec>,
  pub pid_in_msg: Option<StyleSpec>,
  pub comm: Option<StyleSpec>,
  pub tracer_info: Option<StyleSpec>,
  pub tracer_warning: Option<StyleSpec>,
  pub tracer_error: Option<StyleSpec>,
  pub new_child_pid: Option<StyleSpec>,
  pub tracer_event: Option<StyleSpec>,
  pub inline_tracer_error: Option<StyleSpec>,
  pub filename: Option<StyleSpec>,
  pub modified_fd_in_cmdline: Option<StyleSpec>,
  pub removed_fd_in_cmdline: Option<StyleSpec>,
  pub cloexec_fd_in_cmdline: Option<StyleSpec>,
  pub added_fd_in_cmdline: Option<StyleSpec>,
  pub arg0: Option<StyleSpec>,
  pub cwd: Option<StyleSpec>,
  pub deleted_env_var: Option<StyleSpec>,
  pub modified_env_var: Option<StyleSpec>,
  pub added_env_var: Option<StyleSpec>,
  pub argv: Option<StyleSpec>,
  pub search_match: Option<StyleSpec>,
  pub query_no_match: Option<StyleSpec>,
  pub query_match_current_no: Option<StyleSpec>,
  pub query_match_total_cnt: Option<StyleSpec>,
  pub empty_field: Option<StyleSpec>,
  pub uid_gid_name: Option<StyleSpec>,
  pub uid_gid_value: Option<StyleSpec>,
  pub exec_result_success: Option<StyleSpec>,
  pub exec_result_failure: Option<StyleSpec>,
  pub value_unknown: Option<StyleSpec>,
  pub fd_closed: Option<StyleSpec>,
  pub plus_sign: Option<StyleSpec>,
  pub minus_sign: Option<StyleSpec>,
  pub equal_sign: Option<StyleSpec>,
  pub added_env_key: Option<StyleSpec>,
  pub added_env_val: Option<StyleSpec>,
  pub removed_env_key: Option<StyleSpec>,
  pub removed_env_val: Option<StyleSpec>,
  pub unchanged_env_key: Option<StyleSpec>,
  pub unchanged_env_val: Option<StyleSpec>,
  pub fd_label: Option<StyleSpec>,
  pub fd_number_label: Option<StyleSpec>,
  pub sublabel: Option<StyleSpec>,
  pub selected_label: Option<StyleSpec>,
  pub label: Option<StyleSpec>,
  pub selection_indicator: Option<StyleSpec>,
  pub open_flag_cloexec: Option<StyleSpec>,
  pub open_flag_access_mode: Option<StyleSpec>,
  pub open_flag_creation: Option<StyleSpec>,
  pub open_flag_status: Option<StyleSpec>,
  pub open_flag_other: Option<StyleSpec>,
  pub visual_separator: Option<StyleSpec>,
  pub error_popup: Option<StyleSpec>,
  pub info_popup: Option<StyleSpec>,
  pub active_tab: Option<StyleSpec>,
  pub status_process_running: Option<StyleSpec>,
  pub status_process_paused: Option<StyleSpec>,
  pub status_process_detached: Option<StyleSpec>,
  pub status_exec_error: Option<StyleSpec>,
  pub status_process_exited_normally: Option<StyleSpec>,
  pub status_process_exited_abnormally: Option<StyleSpec>,
  pub status_process_killed: Option<StyleSpec>,
  pub status_process_terminated: Option<StyleSpec>,
  pub status_process_interrupted: Option<StyleSpec>,
  pub status_process_segfault: Option<StyleSpec>,
  pub status_process_aborted: Option<StyleSpec>,
  pub status_process_sigill: Option<StyleSpec>,
  pub status_process_signaled: Option<StyleSpec>,
  pub status_internal_failure: Option<StyleSpec>,
  pub breakpoint_title_selected: Option<StyleSpec>,
  pub breakpoint_title: Option<StyleSpec>,
  pub breakpoint_pattern_type_label: Option<StyleSpec>,
  pub breakpoint_pattern: Option<StyleSpec>,
  pub breakpoint_info_label: Option<StyleSpec>,
  pub breakpoint_info_label_active: Option<StyleSpec>,
  pub breakpoint_info_value: Option<StyleSpec>,
  pub hit_entry_pid: Option<StyleSpec>,
  pub hit_entry_plain_text: Option<StyleSpec>,
  pub hit_entry_breakpoint_stop: Option<StyleSpec>,
  pub hit_entry_breakpoint_pattern: Option<StyleSpec>,
  pub hit_entry_no_breakpoint_pattern: Option<StyleSpec>,
  pub hit_manager_default_command: Option<StyleSpec>,
  pub hit_manager_no_default_command: Option<StyleSpec>,
  pub backtrace_parent_spawns: Option<SpanSpec>,
  pub backtrace_parent_becomes: Option<SpanSpec>,
  pub backtrace_parent_unknown: Option<SpanSpec>,
  /// Collects unrecognised top-level theme keys so the caller can warn about them.
  #[serde(flatten)]
  pub extra: HashMap<String, toml::Value>,
}

impl ThemeSpec {
  /// Emit `tracing::warn!` for every unknown field in this spec and in all
  /// nested `StyleSpec` / `SpanSpec` values.
  ///
  /// `source` is a human-readable description of where the spec came from
  /// (e.g. a file path or `"inline theme config"`).
  pub fn warn_unknown_fields(&self, source: &str, section: Option<&str>) {
    for key in self.extra.keys() {
      if let Some(section) = section {
        tracing::warn!("Unknown key '{key}' in [{section}] in {source} will be ignored");
      } else {
        tracing::warn!("Unknown key '{key}' in {source} will be ignored");
      }
    }

    macro_rules! warn_style {
      ($($field:ident),* $(,)?) => {
        $(
          if let Some(ref spec) = self.$field {
            let key = kebab_case(stringify!($field));
            spec.warn_unknown_fields(source, &key, section);
          }
        )*
      };
    }
    warn_style!(
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
      hit_manager_no_default_command,
    );

    macro_rules! warn_span {
      ($($field:ident),* $(,)?) => {
        $(
          if let Some(ref spec) = self.$field {
            let key = kebab_case(stringify!($field));
            spec.style.warn_unknown_fields(source, &key, section);
          }
        )*
      };
    }
    warn_span!(
      backtrace_parent_spawns,
      backtrace_parent_becomes,
      backtrace_parent_unknown,
    );
  }
}

/// Partial style override: only fields that are `Some` are applied on top of
/// the existing style.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StyleSpec {
  pub fg: Option<ThemeColor>,
  pub bg: Option<ThemeColor>,
  pub underline_color: Option<ThemeColor>,
  #[serde(default)]
  pub modifiers: Vec<ThemeModifier>,
  #[serde(default)]
  pub remove_modifiers: Vec<ThemeModifier>,
  /// Collects unrecognised keys for user-visible warnings.
  #[serde(flatten)]
  pub extra: HashMap<String, toml::Value>,
}

impl StyleSpec {
  pub fn warn_unknown_fields(&self, source: &str, parent_key: &str, section: Option<&str>) {
    for unknown_key in self.extra.keys() {
      if let Some(section) = section {
        tracing::warn!(
          "Unknown key '{unknown_key}' for '{parent_key}' in [{section}] in {source} will be ignored"
        );
      } else {
        tracing::warn!(
          "Unknown key '{unknown_key}' for '{parent_key}' in {source} will be ignored"
        );
      }
    }
  }
}

fn kebab_case(field: &str) -> String {
  field.replace('_', "-")
}

/// Partial span override: `content` replaces the span text; all style fields
/// are forwarded to `StyleSpec`.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SpanSpec {
  pub content: Option<String>,
  #[serde(flatten)]
  pub style: StyleSpec,
}

/// A color value as it appears in a theme TOML file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ThemeColor {
  Named(String),
  Indexed(u8),
  Rgb { r: u8, g: u8, b: u8 },
}

/// A modifier flag that can be listed under `modifiers` or `remove-modifiers`.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeModifier {
  Bold,
  Dim,
  Italic,
  Underlined,
  SlowBlink,
  RapidBlink,
  Reversed,
  Hidden,
  CrossedOut,
}

/// Top-level wrapper for a theme TOML file.
///
/// All TUI theme keys must live inside the `[tui]` section.  This makes the
/// format forward-compatible so that future `[log]` (or other mode) sections
/// can coexist in the same file without conflicts.
///
/// ```toml
/// [tui]
/// active-border = { fg = "cyan", modifiers = ["bold"] }
/// ```
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ThemeFile {
  /// TUI-mode theme overrides.
  pub tui: Option<ThemeSpec>,
  /// Collects unrecognised top-level sections so the caller can warn.
  #[serde(flatten)]
  pub extra: HashMap<String, toml::Value>,
}

impl ThemeFile {
  /// Emit `tracing::warn!` for every unknown top-level section in the file.
  pub fn warn_unknown_sections(&self, source: &str) {
    for key in self.extra.keys() {
      tracing::warn!(
        "Unknown key '{key}' at top level in theme file {source} will be ignored; theme keys must be under [tui]"
      );
    }
  }
}
