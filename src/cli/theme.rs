use std::sync::LazyLock;

use owo_colors::Style;

pub struct Theme {
  pub inline_error: Style,
  // Env
  pub removed_env_var: Style,
  pub removed_env_key: Style,
  pub added_env_var: Style,
  pub modified_env_key: Style,
  pub modified_env_val: Style,
  // Info
  pub filename: Style,
  pub cwd: Style,
  // Fd
  pub modified_fd: Style,
  pub added_fd: Style,
  // pub removed_fd: Style,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      inline_error: Style::new().bright_red().bold().blink(),
      removed_env_var: Style::new().bright_red().strikethrough(),
      removed_env_key: Style::new().bright_red(),
      added_env_var: Style::new().green(),
      modified_env_key: Style::new().yellow(),
      modified_env_val: Style::new().bright_blue(),
      filename: Style::new(),
      cwd: Style::new().bright_cyan(),
      modified_fd: Style::new().bright_yellow().bold(),
      added_fd: Style::new().bright_green().bold(),
    }
  }
}

pub static THEME: LazyLock<Theme> = LazyLock::new(Default::default);
