use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Display, Deserialize, Serialize,
)]
#[strum(serialize_all = "kebab-case")]
pub enum AppLayout {
  #[default]
  Horizontal,
  Vertical,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum Color {
  Auto,
  Always,
  Never,
}

#[derive(
  Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Display, Default, Deserialize, Serialize,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SeccompBpf {
  #[default]
  Auto,
  On,
  Off,
}

#[derive(
  Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Display, Default, Deserialize, Serialize,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ActivePane {
  #[default]
  Terminal,
  Events,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Display, Deserialize, Serialize)]
#[strum(serialize_all = "kebab-case")]
pub enum ExportFormat {
  // https://jsonlines.org/
  JsonStream,
  Json,
  // https://clang.llvm.org/docs/JSONCompilationDatabase.html
  // CompileCommands,
  Perfetto,
}
