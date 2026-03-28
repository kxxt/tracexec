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
  theme::Theme,
};

mod private {
  use tracexec_core::event::OutputMsg;

  pub trait Sealed {}

  impl Sealed for OutputMsg {}
}

pub trait OutputMsgTuiExt: Sealed {
  fn bash_escaped_with_style(&self, style: Style, theme: &Theme) -> Span<'static>;
  #[allow(unused)]
  fn styled(&self, style: Style, theme: &Theme) -> Span<'_>;
}

impl OutputMsgTuiExt for OutputMsg {
  fn bash_escaped_with_style(&self, style: Style, theme: &Theme) -> Span<'static> {
    match self {
      Self::Ok(s) => {
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash).set_style(style)
      }
      Self::PartialOk(s) => {
        shell_quote::QuoteRefExt::<String>::quoted(s.as_str(), shell_quote::Bash)
          .set_style(style)
          .patch_style(theme.inline_tracer_error)
      }
      Self::Err(e) => <&'static str>::from(e).set_style(theme.inline_tracer_error),
    }
  }

  fn styled(&self, style: Style, theme: &Theme) -> Span<'_> {
    match self {
      Self::Ok(s) => (*s).set_style(style),
      Self::PartialOk(s) => (*s).set_style(theme.inline_tracer_error),
      Self::Err(e) => <&'static str>::from(e).set_style(theme.inline_tracer_error),
    }
  }
}

#[cfg(test)]
mod tests {
  use nix::errno::Errno;
  use ratatui::style::Style;
  use tracexec_core::event::{
    FriendlyError,
    OutputMsg,
  };

  use super::OutputMsgTuiExt;
  use crate::theme::current_theme;

  #[test]
  fn bash_escaped_with_style_quotes_ok_output() {
    let msg = OutputMsg::Ok("hello world".into());
    let span = msg.bash_escaped_with_style(Style::default(), current_theme());
    let expected = shell_quote::QuoteRefExt::<String>::quoted("hello world", shell_quote::Bash);
    assert_eq!(span.content.as_ref(), expected);
  }

  #[test]
  fn bash_escaped_with_style_marks_partial_ok_as_error() {
    let msg = OutputMsg::PartialOk("oops".into());
    let span = msg.bash_escaped_with_style(Style::default(), current_theme());
    assert_eq!(span.content.as_ref(), "oops");
    assert_eq!(span.style, current_theme().inline_tracer_error);
  }

  #[test]
  fn styled_err_uses_friendly_error_string() {
    let msg = OutputMsg::Err(FriendlyError::InspectError(Errno::EPERM));
    let span = msg.styled(Style::default(), current_theme());
    assert_eq!(span.content.as_ref(), "[err: failed to inspect]");
  }
}
