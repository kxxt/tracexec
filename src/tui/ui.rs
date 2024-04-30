use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::Stylize,
  text::Text,
  widgets::{Paragraph, Widget},
};

pub fn render_title<'a>(area: Rect, buf: &mut Buffer, title: impl Into<Text<'a>>) {
  Paragraph::new(title).bold().render(area, buf);
}
