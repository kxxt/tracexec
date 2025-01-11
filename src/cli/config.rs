use std::{io, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::tui::app::AppLayout;

use super::options::{ActivePane, SeccompBpf};

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
  pub log: Option<LogModeConfig>,
  pub tui: Option<TuiModeConfig>,
  pub modifier: Option<ModifierConfig>,
  pub ptrace: Option<PtraceConfig>,
  pub debugger: Option<DebuggerConfig>,
}

#[derive(Debug, Error)]
pub enum ConfigLoadError {
  #[error("Config file not found.")]
  NotFound,
  #[error("Failed to load config file.")]
  IoError(#[from] io::Error),
  #[error("Failed to parse config file.")]
  TomlError(#[from] toml::de::Error),
}

impl Config {
  pub fn load(path: Option<PathBuf>) -> Result<Self, ConfigLoadError> {
    let config_text = match path {
      Some(path) => std::fs::read_to_string(path)?, // if manually specified config doesn't exist, return a hard error
      None => {
        let Some(project_dirs) = project_directory() else {
          warn!("No valid home directory found! Not loading config.toml.");
          return Err(ConfigLoadError::NotFound);
        };
        // ~/.config/tracexec/config.toml
        let config_path = project_dirs.config_dir().join("config.toml");

        std::fs::read_to_string(config_path).map_err(|e| match e.kind() {
          io::ErrorKind::NotFound => ConfigLoadError::NotFound,
          _ => ConfigLoadError::from(e),
        })?
      }
    };

    let config: Self = toml::from_str(&config_text)?;
    Ok(config)
  }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ModifierConfig {
  pub seccomp_bpf: Option<SeccompBpf>,
  pub successful_only: Option<bool>,
  pub fd_in_cmdline: Option<bool>,
  pub stdio_in_cmdline: Option<bool>,
  pub resolve_proc_self_exe: Option<bool>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct PtraceConfig {
  pub seccomp_bpf: Option<SeccompBpf>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct TuiModeConfig {
  pub follow: Option<bool>,
  pub exit_handling: Option<ExitHandling>,
  pub active_pane: Option<ActivePane>,
  pub layout: Option<AppLayout>,
  #[serde(default, deserialize_with = "deserialize_frame_rate")]
  pub frame_rate: Option<f64>,
  pub max_events: Option<u64>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct DebuggerConfig {
  pub default_external_command: Option<String>,
}

fn deserialize_frame_rate<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
  D: Deserializer<'de>,
{
  let value = Option::<f64>::deserialize(deserializer)?;
  if value.is_some_and(|v| v.is_nan() || v <= 0. || v.is_infinite()) {
    return Err(serde::de::Error::invalid_value(
      serde::de::Unexpected::Float(value.unwrap()),
      &"a positive floating-point number",
    ));
  }
  Ok(value)
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct LogModeConfig {
  pub show_interpreter: Option<bool>,
  pub color_level: Option<ColorLevel>,
  pub foreground: Option<bool>,
  pub fd_display: Option<FileDescriptorDisplay>,
  pub env_display: Option<EnvDisplay>,
  pub show_comm: Option<bool>,
  pub show_argv: Option<bool>,
  pub show_filename: Option<bool>,
  pub show_cwd: Option<bool>,
  pub show_cmdline: Option<bool>,
  pub decode_errno: Option<bool>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum ColorLevel {
  Less,
  #[default]
  Normal,
  More,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum FileDescriptorDisplay {
  Hide,
  Show,
  #[default]
  Diff,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum EnvDisplay {
  Hide,
  Show,
  #[default]
  Diff,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum ExitHandling {
  #[default]
  Wait,
  Kill,
  Terminate,
}

pub fn project_directory() -> Option<ProjectDirs> {
  ProjectDirs::from("dev", "kxxt", env!("CARGO_PKG_NAME"))
}
