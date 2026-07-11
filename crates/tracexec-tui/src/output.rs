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
  #[allow(unused)]
  fn styled(&self, style: Style, theme: &Theme) -> Span<'static>;
}

impl OutputMsgTuiExt for OutputMsg {
  fn styled(&self, style: Style, theme: &Theme) -> Span<'static> {
    match self {
      Self::Ok(s) => s.to_string().set_style(style),
      Self::PartialOk(s) => s.to_string().set_style(style).patch_style(theme.partial_ok),
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
  fn styled_partial_ok_keeps_base_style_and_marks_partial() {
    let msg = OutputMsg::PartialOk("partial/path".into());
    let base = Style::default().green();
    let span = msg.styled(base, current_theme());
    assert_eq!(span.content.as_ref(), "partial/path");
    assert_eq!(span.style, base.patch(current_theme().partial_ok));
  }

  #[test]
  fn styled_err_uses_friendly_error_string() {
    let msg = OutputMsg::Err(FriendlyError::InspectError(Errno::EPERM));
    let span = msg.styled(Style::default(), current_theme());
    assert_eq!(span.content.as_ref(), "[err: failed to inspect]");
  }
}
