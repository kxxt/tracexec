use ratatui::{
  style::{
    Style,
    Styled,
  },
  text::Span,
};
use tracexec_core::event::OutputMsg;

use crate::{
  output::private::Sealed,
  theme::THEME,
};

mod private {
  use tracexec_core::event::OutputMsg;

  pub trait Sealed {}

  impl Sealed for OutputMsg {}
}

pub trait OutputMsgTuiExt: Sealed {
  fn bash_escaped_with_style(&self, style: Style) -> Span<'static>;
  #[allow(unused)]
  fn styled(&self, style: Style) -> Span<'_>;
}

impl OutputMsgTuiExt for OutputMsg {
  fn bash_escaped_with_style(&self, style: Style) -> Span<'static> {
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

  fn styled(&self, style: Style) -> Span<'_> {
    match self {
      Self::Ok(s) => (*s).set_style(style),
      Self::PartialOk(s) => (*s).set_style(THEME.inline_tracer_error),
      Self::Err(e) => <&'static str>::from(e).set_style(THEME.inline_tracer_error),
    }
  }
}
