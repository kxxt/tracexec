use std::{
  io,
  path::PathBuf,
};

use directories::ProjectDirs;
use serde::{
  Deserialize,
  Deserializer,
  Serialize,
};
use snafu::{
  ResultExt,
  Snafu,
};
use tracing::warn;

use super::options::{
  ActivePane,
  AppLayout,
  SeccompBpf,
};
use crate::timestamp::TimestampFormat;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Config {
  pub log: Option<LogModeConfig>,
  pub tui: Option<TuiModeConfig>,
  pub modifier: Option<ModifierConfig>,
  pub ptrace: Option<PtraceConfig>,
  pub debugger: Option<DebuggerConfig>,
}

#[derive(Debug, Snafu)]
pub enum ConfigLoadError {
  #[snafu(display("Config file not found."))]
  NotFound,
  #[snafu(display("Failed to load config file."))]
  IoError { source: io::Error },
  #[snafu(display("Failed to parse config file."))]
  TomlError { source: toml::de::Error },
}

impl Config {
  pub fn load(path: Option<PathBuf>) -> Result<Self, ConfigLoadError> {
    let config_text = match path {
      Some(path) => std::fs::read_to_string(path).context(IoSnafu)?, // if manually specified config doesn't exist, return a hard error
      None => {
        let Some(project_dirs) = project_directory() else {
          warn!("No valid home directory found! Not loading config.toml.");
          return Err(ConfigLoadError::NotFound);
        };
        // ~/.config/tracexec/config.toml
        let config_path = project_dirs.config_dir().join("config.toml");

        std::fs::read_to_string(config_path).map_err(|e| match e.kind() {
          io::ErrorKind::NotFound => ConfigLoadError::NotFound,
          _ => ConfigLoadError::IoError { source: e },
        })?
      }
    };

    let config: Self = toml::from_str(&config_text).context(TomlSnafu)?;
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
  pub hide_cloexec_fds: Option<bool>,
  pub timestamp: Option<TimestampConfig>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct TimestampConfig {
  pub enable: bool,
  pub inline_format: Option<TimestampFormat>,
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

fn is_frame_rate_invalid(v: f64) -> bool {
  v.is_nan() || v <= 0. || v.is_infinite()
}

fn deserialize_frame_rate<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
  D: Deserializer<'de>,
{
  let value = Option::<f64>::deserialize(deserializer)?;
  if value.is_some_and(is_frame_rate_invalid) {
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

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum ColorLevel {
  Less,
  #[default]
  Normal,
  More,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum FileDescriptorDisplay {
  Hide,
  Show,
  #[default]
  Diff,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum EnvDisplay {
  Hide,
  Show,
  #[default]
  Diff,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum ExitHandling {
  #[default]
  Wait,
  Kill,
  Terminate,
}

pub fn project_directory() -> Option<ProjectDirs> {
  ProjectDirs::from("dev", "kxxt", env!("CARGO_PKG_NAME"))
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use toml;

  use super::*;

  #[test]
  fn test_validate_frame_rate() {
    // valid frame rates
    assert!(!is_frame_rate_invalid(5.0));
    assert!(!is_frame_rate_invalid(12.5));

    // too low or zero
    assert!(is_frame_rate_invalid(0.0));
    assert!(is_frame_rate_invalid(-1.0));

    // NaN or infinite
    assert!(is_frame_rate_invalid(f64::NAN));
    assert!(is_frame_rate_invalid(f64::INFINITY));
    assert!(is_frame_rate_invalid(f64::NEG_INFINITY));
  }

  #[derive(Serialize, Deserialize)]
  struct FrameRate {
    #[serde(default, deserialize_with = "deserialize_frame_rate")]
    frame_rate: Option<f64>,
  }

  #[test]
  fn test_deserialize_frame_rate_valid() {
    let value: FrameRate = toml::from_str("frame_rate = 12.5").unwrap();
    assert_eq!(value.frame_rate, Some(12.5));

    let value: FrameRate = toml::from_str("frame_rate = 5.0").unwrap();
    assert_eq!(value.frame_rate, Some(5.0));
  }

  #[test]
  fn test_deserialize_frame_rate_invalid() {
    let value: Result<FrameRate, _> = toml::from_str("frame_rate = -1");
    assert!(value.is_err());

    let value: Result<FrameRate, _> = toml::from_str("frame_rate = NaN");
    assert!(value.is_err());

    let value: Result<FrameRate, _> = toml::from_str("frame_rate = 0");
    assert!(value.is_err());
  }

  #[test]
  fn test_config_load_invalid_path() {
    let path = Some(PathBuf::from("/non/existent/config.toml"));
    let result = Config::load(path);
    assert!(matches!(
      result,
      Err(ConfigLoadError::IoError { .. }) | Err(ConfigLoadError::NotFound)
    ));
  }

  #[test]
  fn test_modifier_config_roundtrip() {
    let toml_str = r#"
seccomp_bpf = "Auto"
successful_only = true
fd_in_cmdline = false
stdio_in_cmdline = true
resolve_proc_self_exe = true
hide_cloexec_fds = false

[timestamp]
enable = true
inline_format = "%H:%M:%S"
"#;

    let cfg: ModifierConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.successful_only.unwrap(), true);
    assert_eq!(cfg.stdio_in_cmdline.unwrap(), true);
    assert_eq!(cfg.timestamp.as_ref().unwrap().enable, true);
    assert_eq!(
      cfg
        .timestamp
        .as_ref()
        .unwrap()
        .inline_format
        .as_ref()
        .unwrap()
        .as_str(),
      "%H:%M:%S"
    );
  }

  #[test]
  fn test_ptrace_config_roundtrip() {
    let toml_str = r#"seccomp_bpf = "Auto""#;
    let cfg: PtraceConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.seccomp_bpf.unwrap(), SeccompBpf::Auto);
  }

  #[test]
  fn test_log_mode_config_roundtrip() {
    let toml_str = r#"
show_interpreter = true
color_level = "More"
foreground = false
"#;
    let cfg: LogModeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.show_interpreter.unwrap(), true);
    assert_eq!(cfg.color_level.unwrap(), ColorLevel::More);
    assert_eq!(cfg.foreground.unwrap(), false);
  }

  #[test]
  fn test_tui_mode_config_roundtrip() {
    let toml_str = r#"
follow = true
frame_rate = 12.5
max_events = 100
"#;
    let cfg: TuiModeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.follow.unwrap(), true);
    assert_eq!(cfg.frame_rate.unwrap(), 12.5);
    assert_eq!(cfg.max_events.unwrap(), 100);
  }

  #[test]
  fn test_debugger_config_roundtrip() {
    let toml_str = r#"default_external_command = "echo hello""#;
    let cfg: DebuggerConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.default_external_command.unwrap(), "echo hello");
  }
}
