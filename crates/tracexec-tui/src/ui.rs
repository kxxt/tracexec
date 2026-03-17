use ratatui::{
  buffer::Buffer,
  layout::Rect,
  text::Text,
  widgets::{
    Paragraph,
    Widget,
  },
};

use super::theme::THEME;

pub fn render_title<'a>(area: Rect, buf: &mut Buffer, title: impl Into<Text<'a>>) {
  Paragraph::new(title)
    .style(THEME.app_title)
    .render(area, buf);
}

#[cfg(test)]
mod tests {
  use insta::assert_snapshot;
  use ratatui::{
    Terminal,
    backend::TestBackend,
  };

  use super::render_title;

  #[test]
  fn snapshot_render_title() {
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).unwrap();
    terminal
      .draw(|frame| {
        render_title(frame.area(), frame.buffer_mut(), "tracexec");
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }
}
