use clap::ValueEnum;
use strum::Display;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum Color {
  Auto,
  Always,
  Never,
}

#[cfg(feature = "seccomp-bpf")]
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Display)]
#[strum(serialize_all = "kebab-case")]
pub enum SeccompBpf {
  Auto,
  On,
  Off,
}
