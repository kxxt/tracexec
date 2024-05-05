use lazy_static::lazy_static;
use ratatui::style::{Style, Stylize};

pub struct Theme {
  // Color for UI Elements
  pub inactive_border: Style,
  pub active_border: Style,
  pub popup_border: Style,
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
  pub added_fd_in_cmdline: Style,
  pub arg0: Style,
  pub cwd: Style,
  pub deleted_env_var: Style,
  pub modified_env_var: Style,
  pub added_env_var: Style,
  pub argv: Style,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      inactive_border: Style::default(),
      active_border: Style::default(),
      popup_border: Style::default(),
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
      arg0: Style::default().white().italic(),
      cwd: Style::default().light_cyan(),
      deleted_env_var: Style::default().light_red(),
      modified_env_var: Style::default().yellow(),
      added_env_var: Style::default().green(),
      argv: Style::default(),
    }
  }
}

lazy_static! {
  pub static ref THEME: Theme = Theme::default();
}
