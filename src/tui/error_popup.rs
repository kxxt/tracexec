use crossterm::event::KeyEvent;
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Style, Stylize},
  text::Line,
  widgets::{Paragraph, StatefulWidget, WidgetRef, Wrap},
};
use tui_popup::Popup;

use crate::action::Action;

use super::{sized_paragraph::SizedParagraph, theme::THEME};

#[derive(Debug, Clone)]
pub struct InfoPopupState {
  pub title: String,
  pub message: Vec<Line<'static>>,
  pub style: Style,
}

impl InfoPopupState {
  pub fn handle_key_event(&self, _key: KeyEvent) -> Option<Action> {
    Some(Action::CancelCurrentPopup)
  }

  pub fn error(title: String, message: Vec<Line<'static>>) -> Self {
    Self {
      title,
      message,
      style: THEME.error_popup,
    }
  }

  pub fn info(title: String, message: Vec<Line<'static>>) -> Self {
    Self {
      title,
      message,
      style: THEME.info_popup,
    }
  }
}

#[derive(Debug, Clone)]
pub struct InfoPopup;

impl StatefulWidget for InfoPopup {
  type State = InfoPopupState;

  fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    let help = Line::raw("Press any key to close this popup");
    let mut message = state.message.clone();
    message.push("".into());
    message.push(help.centered().bold());
    let paragraph = Paragraph::new(message).wrap(Wrap { trim: false });
    let popup = Popup::new(SizedParagraph::new(
      paragraph,
      (area.width as f32 * 0.7) as usize,
    ))
    .title(Line::raw(state.title.as_str()).centered())
    .style(state.style);
    popup.render_ref(area, buf);
  }
}
