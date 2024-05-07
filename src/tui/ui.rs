use ratatui::{
  buffer::Buffer,
  layout::Rect,
  text::Text,
  widgets::{Paragraph, Widget},
};

use super::theme::THEME;

pub fn render_title<'a>(area: Rect, buf: &mut Buffer, title: impl Into<Text<'a>>) {
  Paragraph::new(title)
    .style(THEME.app_title)
    .render(area, buf);
}
