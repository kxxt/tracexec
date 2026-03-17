use ratatui::{
  buffer::Buffer,
  layout::Rect,
  widgets::{
    Paragraph,
    Widget,
    WidgetRef,
  },
};
use tui_popup::KnownSize;

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

impl KnownSize for SizedParagraph<'_> {
  fn width(&self) -> usize {
    self.width
  }

  fn height(&self) -> usize {
    self.inner.line_count(self.width as u16)
  }
}

impl WidgetRef for SizedParagraph<'_> {
  fn render_ref(&self, area: Rect, buf: &mut Buffer) {
    (&self.inner).render_ref(area, buf)
  }
}

impl Widget for SizedParagraph<'_> {
  fn render(self, area: Rect, buf: &mut Buffer) {
    self.inner.render(area, buf)
  }
}

#[cfg(test)]
mod tests {
  use ratatui::{
    text::Text,
    widgets::Paragraph,
  };
  use tui_popup::KnownSize;

  use super::SizedParagraph;

  #[test]
  fn sized_paragraph_reports_dimensions() {
    let paragraph = Paragraph::new(Text::from("alpha\nbeta"));
    let sized = SizedParagraph::new(paragraph, 40);
    assert_eq!(sized.width(), 40);
    assert_eq!(sized.height(), 2);
  }
}
