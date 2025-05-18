use std::{
  borrow::Cow,
  fmt::{Debug, Display},
  hash::Hash,
};

use crate::cache::ArcStr;
use either::Either;
use nix::errno::Errno;
use owo_colors::OwoColorize;
use ratatui::{
  style::{Style, Styled},
  text::Span,
};
use serde::Serialize;

use crate::{
  cli::{self},
  proc::cached_string,
  tui::theme::THEME,
};

#[cfg(feature = "ebpf")]
use crate::bpf::BpfError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum FriendlyError {
  InspectError(Errno),
  #[cfg(feature = "ebpf")]
  Bpf(BpfError),
}

impl PartialOrd for FriendlyError {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(Ord::cmp(self, other))
  }
}

impl Ord for FriendlyError {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    match (self, other) {
      (Self::InspectError(a), Self::InspectError(b)) => (*a as i32).cmp(&(*b as i32)),
      #[cfg(feature = "ebpf")]
      (Self::Bpf(a), Self::Bpf(b)) => a.cmp(b),
      #[cfg(feature = "ebpf")]
      (Self::InspectError(_), Self::Bpf(_)) => std::cmp::Ordering::Less,
      #[cfg(feature = "ebpf")]
      (Self::Bpf(_), Self::InspectError(_)) => std::cmp::Ordering::Greater,
    }
  }
}

impl Hash for FriendlyError {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    core::mem::discriminant(self).hash(state);
    match self {
      Self::InspectError(e) => (*e as i32).hash(state),
      #[cfg(feature = "ebpf")]
      Self::Bpf(e) => e.hash(state),
    }
  }
}

#[cfg(feature = "ebpf")]
impl From<BpfError> for FriendlyError {
  fn from(value: BpfError) -> Self {
    Self::Bpf(value)
  }
}

impl From<&FriendlyError> for &'static str {
  fn from(value: &FriendlyError) -> Self {
    match value {
      FriendlyError::InspectError(_) => "[err: failed to inspect]",
      #[cfg(feature = "ebpf")]
      FriendlyError::Bpf(_) => "[err: bpf error]",
    }
  }
}

// we need to implement custom Display so Result and Either do not fit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OutputMsg {
  Ok(ArcStr),
  // Part of the message contains error
  PartialOk(ArcStr),
  Err(FriendlyError),
}

impl AsRef<str> for OutputMsg {
  fn as_ref(&self) -> &str {
    match self {
      Self::Ok(s) => s.as_ref(),
      Self::PartialOk(s) => s.as_ref(),
      Self::Err(e) => <&'static str>::from(e),
    }
  }
}

impl Serialize for OutputMsg {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    match self {
      Self::Ok(s) => s.serialize(serializer),
      Self::PartialOk(s) => s.serialize(serializer),
      Self::Err(e) => <&'static str>::from(e).serialize(serializer),
    }
  }
}

impl Display for OutputMsg {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Ok(msg) => write!(f, "{msg:?}"),
      Self::PartialOk(msg) => write!(f, "{:?}", cli::theme::THEME.inline_error.style(msg)),
      Self::Err(e) => Display::fmt(&cli::theme::THEME.inline_error.style(&e), f),
    }
  }
}

impl Display for FriendlyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", <&'static str>::from(self))
  }
}

impl From<ArcStr> for OutputMsg {
  fn from(value: ArcStr) -> Self {
    Self::Ok(value)
  }
}

impl OutputMsg {
  pub fn not_ok(&self) -> bool {
    !matches!(self, Self::Ok(_))
  }

  pub fn is_ok_and(&self, predicate: impl FnOnce(&str) -> bool) -> bool {
    match self {
      Self::Ok(s) => predicate(s),
      Self::PartialOk(_) => false,
      Self::Err(_) => false,
    }
  }

  pub fn is_err_or(&self, predicate: impl FnOnce(&str) -> bool) -> bool {
    match self {
      Self::Ok(s) => predicate(s),
      Self::PartialOk(_) => true,
      Self::Err(_) => true,
    }
  }

  /// Join two paths with a '/', preserving the semantics of [`OutputMsg`]
  pub fn join(&self, path: impl AsRef<str>) -> Self {
    let path = path.as_ref();
    match self {
      Self::Ok(s) => Self::Ok(cached_string(format!("{s}/{path}"))),
      Self::PartialOk(s) => Self::PartialOk(cached_string(format!("{s}/{path}"))),
      Self::Err(s) => Self::PartialOk(cached_string(format!("{}/{path}", <&'static str>::from(s)))),
    }
  }

  /// Escape the content for bash shell if it is not error
  pub fn tui_bash_escaped_with_style(&self, style: Style) -> Span<'static> {
    match self {
      Self::Ok(s) => {
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash).set_style(style)
      }
      Self::PartialOk(s) => {
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash)
          .set_style(style)
          .patch_style(THEME.inline_tracer_error)
      }
      Self::Err(e) => <&'static str>::from(e).set_style(THEME.inline_tracer_error),
    }
  }

  /// Escape the content for bash shell if it is not error
  pub fn cli_bash_escaped_with_style(
    &self,
    style: owo_colors::Style,
  ) -> Either<impl Display, impl Display> {
    match self {
      Self::Ok(s) => Either::Left(style.style(shell_quote::QuoteRefExt::<String>::quoted(
        s.as_str(),
        shell_quote::Bash,
      ))),
      Self::PartialOk(s) => Either::Left(cli::theme::THEME.inline_error.style(
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash),
      )),
      Self::Err(e) => Either::Right(
        cli::theme::THEME
          .inline_error
          .style(<&'static str>::from(e)),
      ),
    }
  }

  /// Escape the content for bash shell if it is not error
  pub fn bash_escaped(&self) -> Cow<'static, str> {
    match self {
      Self::Ok(s) | Self::PartialOk(s) => Cow::Owned(shell_quote::QuoteRefExt::quoted(
        s.as_str(),
        shell_quote::Bash,
      )),
      Self::Err(e) => Cow::Borrowed(<&'static str>::from(e)),
    }
  }

  pub fn tui_styled(&self, style: Style) -> Span {
    match self {
      Self::Ok(s) => (*s).set_style(style),
      Self::PartialOk(s) => (*s).set_style(THEME.inline_tracer_error),
      Self::Err(e) => <&'static str>::from(e).set_style(THEME.inline_tracer_error),
    }
  }

  pub fn cli_styled(&self, style: owo_colors::Style) -> Either<impl Display + '_, impl Display> {
    match self {
      Self::Ok(s) => Either::Left(s.style(style)),
      Self::PartialOk(s) => Either::Left(s.style(cli::theme::THEME.inline_error)),
      Self::Err(e) => Either::Right(
        cli::theme::THEME
          .inline_error
          .style(<&'static str>::from(e)),
      ),
    }
  }

  pub fn cli_escaped_styled(
    &self,
    style: owo_colors::Style,
  ) -> Either<impl Display + '_, impl Display> {
    // We (ab)use Rust's Debug feature to escape our string.
    struct DebugAsDisplay<T: Debug>(T);
    impl<T: Debug> Display for DebugAsDisplay<T> {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
      }
    }
    match self {
      Self::Ok(s) => Either::Left(style.style(DebugAsDisplay(s))),
      Self::PartialOk(s) => Either::Left(cli::theme::THEME.inline_error.style(DebugAsDisplay(s))),
      Self::Err(e) => Either::Right(
        cli::theme::THEME
          .inline_error
          .style(<&'static str>::from(e)),
      ),
    }
  }
}
