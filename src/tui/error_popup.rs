use crossterm::event::KeyEvent;
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::Stylize,
  text::Line,
  widgets::{Paragraph, StatefulWidget, WidgetRef, Wrap},
};
use tui_popup::Popup;

use crate::action::Action;

use super::{sized_paragraph::SizedParagraph, theme::THEME};

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
  pub title: String,
  pub message: Vec<Line<'static>>,
}

impl ErrorPopupState {
  pub fn handle_key_event(&mut self, _key: KeyEvent) -> Option<Action> {
    Some(Action::CancelCurrentPopup)
  }
}

#[derive(Debug, Clone)]
pub struct ErrorPopup;

impl StatefulWidget for ErrorPopup {
  type State = ErrorPopupState;

  fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    let help = Line::raw("Press any key to close this popup");
    let mut message = state.message.clone();
    message.push("".into());
    message.push(help.centered().bold());
    let paragraph = Paragraph::new(message).wrap(Wrap { trim: false });
    let popup = Popup::new(
      Line::raw(state.title.as_str()).centered(),
      SizedParagraph::new(paragraph, (area.width as f32 * 0.7) as usize),
    )
    .style(THEME.error_popup);
    popup.render_ref(area, buf);
  }
}
