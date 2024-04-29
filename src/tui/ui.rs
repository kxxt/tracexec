use std::borrow::Cow;

use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Styled, Stylize},
  text::Span,
  widgets::{Paragraph, Widget},
};

pub fn render_title(area: Rect, buf: &mut Buffer, title: &str) {
  Paragraph::new(title).bold().centered().render(area, buf);
}

pub fn help_key<'a, T>(k: T) -> Span<'a>
where
  T: Into<Cow<'a, str>>,
  T: Styled<Item = Span<'a>>,
{
  k.fg(Color::Black).bg(Color::Cyan).bold()
}
pub fn help_desc<'a, T>(d: T) -> Span<'a>
where
  T: Into<Cow<'a, str>>,
  T: Styled<Item = Span<'a>>,
{
  d.fg(Color::Cyan).bg(Color::DarkGray).italic().bold()
}

macro_rules! help_item {
  ($key: literal, $desc: literal) => {{
    let mut key_string = String::from("\u{00a0}");
    key_string.push_str($key);
    key_string.push_str("\u{00a0}");
    let mut desc_string = String::from("\u{00a0}");
    desc_string.push_str($desc);
    desc_string.push_str("\u{00a0}\u{200b}");
    [
      crate::tui::ui::help_key(key_string),
      crate::tui::ui::help_desc(desc_string),
    ]
  }};
}

pub(crate) use help_item;
