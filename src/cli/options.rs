use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum Color {
  Auto,
  Always,
  Never,
}

#[cfg(feature = "seccomp-bpf")]
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display, Default, Deserialize, Serialize)]
#[strum(serialize_all = "kebab-case")]
pub enum SeccompBpf {
  #[default]
  Auto,
  On,
  Off,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display, Default, Deserialize, Serialize)]
#[strum(serialize_all = "kebab-case")]
pub enum ActivePane {
  #[default]
  Terminal,
  Events,
}
