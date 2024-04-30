use ratatui::{
  buffer::Buffer,
  layout::Rect,
  widgets::{Paragraph, WidgetRef},
};
use tui_popup::SizedWidgetRef;

#[derive(Debug)]
pub struct SizedParagraph<'a> {
  inner: Paragraph<'a>,
  width: usize,
}

impl<'a> SizedParagraph<'a> {
  pub fn new(paragraph: Paragraph<'a>, width: usize) -> Self {
    Self {
      inner: paragraph,
      width,
    }
  }
}

impl SizedWidgetRef for SizedParagraph<'_> {
  fn width(&self) -> usize {
    self.width
  }

  fn height(&self) -> usize {
    self.inner.line_count(self.width as u16)
  }
}

impl WidgetRef for SizedParagraph<'_> {
  fn render_ref(&self, area: Rect, buf: &mut Buffer) {
    self.inner.render_ref(area, buf)
  }
}
