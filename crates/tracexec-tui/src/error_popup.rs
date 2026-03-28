use crossterm::event::KeyEvent;
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{
    Style,
    Stylize,
  },
  text::Line,
  widgets::{
    Paragraph,
    StatefulWidget,
    Widget,
    Wrap,
  },
};
use tui_popup::Popup;

use super::sized_paragraph::SizedParagraph;
use crate::{
  action::{
    Action,
    ActivePopup,
  },
  theme::Theme,
};

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

  pub fn error(title: String, message: Vec<Line<'static>>, theme: &Theme) -> Self {
    Self {
      title,
      message,
      style: theme.error_popup,
    }
  }

  pub fn info(title: String, message: Vec<Line<'static>>, theme: &Theme) -> Self {
    Self {
      title,
      message,
      style: theme.info_popup,
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
    Widget::render(popup, area, buf);
  }
}

pub fn err_popup_goto_parent_miss(title: &'static str, theme: &Theme) -> ActivePopup {
  ActivePopup::InfoPopup(InfoPopupState::info(
    title.into(),
    vec![Line::raw(
      "The parent exec event is found, but has been cleared from memory.",
    )],
    theme,
  ))
}

pub fn err_popup_goto_parent_not_found(title: &'static str, theme: &Theme) -> ActivePopup {
  ActivePopup::InfoPopup(InfoPopupState::info(
    title.into(),
    vec![Line::raw("No parent exec event is found for this event.")],
    theme,
  ))
}

pub fn err_popup_goto_parent_not_exec(title: &'static str, theme: &Theme) -> ActivePopup {
  ActivePopup::InfoPopup(InfoPopupState::error(
    title.into(),
    vec![Line::raw(
      "This feature is currently limited to exec events.",
    )],
    theme,
  ))
}

#[cfg(test)]
mod tests {
  use insta::assert_snapshot;
  use ratatui::{
    Terminal,
    backend::TestBackend,
    text::Line,
  };

  use super::{
    InfoPopup,
    InfoPopupState,
  };
  use crate::theme::current_theme;

  #[test]
  fn snapshot_info_popup() {
    let mut state = InfoPopupState::info(
      "Notice".to_string(),
      vec![
        Line::raw("Something went wrong."),
        Line::raw("Please retry."),
      ],
      current_theme(),
    );
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_stateful_widget(InfoPopup, frame.area(), &mut state);
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }
}
