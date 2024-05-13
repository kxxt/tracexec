use lazy_static::lazy_static;
use ratatui::style::{Modifier, Style, Stylize};

pub struct Theme {
  // Color for UI Elements
  pub inactive_border: Style,
  pub active_border: Style,
  pub popup_border: Style,
  pub app_title: Style,
  pub help_popup: Style,
  // Color for help items
  pub cli_flag: Style,
  pub help_key: Style,
  pub help_desc: Style,
  // Tracer Event
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
  // Search & Filter
  pub search_match: Style,
  pub query_no_match: Style,
  pub query_match_current_no: Style,
  pub query_match_total_cnt: Style,
  // Details Popup
  pub exec_result_success: Style,
  pub exec_result_failure: Style,
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
  // Error Popup
  pub error_popup: Style,
  // Tabs
  pub active_tab: Style,
  // Process/Exec Status
  pub status_process_running: &'static str,
  pub status_exec_enoent: &'static str,
  pub status_exec_error: &'static str,
  pub status_process_exited_0: &'static str,
  pub status_process_exited: &'static str,
  pub status_process_killed: &'static str,
  pub status_process_terminated: &'static str,
  pub status_process_interrupted: &'static str,
  pub status_process_segfault: &'static str,
  pub status_process_aborted: &'static str,
  pub status_process_sigill: &'static str,
  pub status_process_signaled: &'static str,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      inactive_border: Style::default().white(),
      active_border: Style::default().cyan(),
      popup_border: Style::default(),
      app_title: Style::default().bold(),
      help_popup: Style::default().black().on_gray(),
      // -- Help Items --
      cli_flag: Style::default().yellow().on_dark_gray().bold(),
      help_key: Style::default().black().on_cyan().bold(),
      help_desc: Style::default()
        .light_green()
        .on_dark_gray()
        .italic()
        .bold(),
      // -- Tracer Event --
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
      // -- Search & Filter --
      search_match: Style::default().add_modifier(Modifier::REVERSED),
      query_no_match: Style::default().light_red(),
      query_match_current_no: Style::default().light_cyan(),
      query_match_total_cnt: Style::default().white(),
      // -- Details Popup --
      exec_result_success: Style::default().green(),
      exec_result_failure: Style::default().red(),
      fd_closed: Style::default().light_red(),
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
      // -- Error Popup --
      error_popup: Style::default().white().on_red(),
      // -- Tabs --
      active_tab: Style::default().white().on_magenta(),
      // -- Process/Exec Status --
      status_process_running: "🟢",
      status_exec_enoent: "⚠️",
      status_exec_error: "❌",
      status_process_exited_0: "😇",
      status_process_exited: "😡",
      status_process_killed: "😵",
      status_process_terminated: "🤬",
      status_process_interrupted: "🥺",
      status_process_segfault: "💥",
      status_process_aborted: "😱",
      status_process_sigill: "👿",
      status_process_signaled: "💀",
    }
  }
}

lazy_static! {
  pub static ref THEME: Theme = Theme::default();
}
