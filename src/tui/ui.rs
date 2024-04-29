use std::borrow::Cow;

use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Styled, Stylize},
  text::{Span, Text},
  widgets::{Paragraph, Widget},
};

pub fn render_title<'a>(area: Rect, buf: &mut Buffer, title: impl Into<Text<'a>>) {
  Paragraph::new(title).bold().render(area, buf);
}

pub fn cli_flag<'a, T>(f: T) -> Span<'a>
where
  T: Into<Cow<'a, str>>,
  T: Styled<Item = Span<'a>>,
{
  f.fg(Color::Yellow).bold()
}

pub fn help_key<'a, T>(k: T) -> Span<'a>
where
  T: Into<Cow<'a, str>>,
  T: Styled<Item = Span<'a>>,
{
  let mut key_string = String::from("\u{00a0}");
  key_string.push_str(&k.into());
  key_string.push_str("\u{00a0}");
  key_string.fg(Color::Black).bg(Color::Cyan).bold()
}
pub fn help_desc<'a, T>(d: T) -> Span<'a>
where
  T: Into<Cow<'a, str>>,
  T: Styled<Item = Span<'a>>,
{
  let mut desc_string = String::from("\u{00a0}");
  desc_string.push_str(&d.into());
  desc_string.push_str("\u{00a0}");
  desc_string
    .fg(Color::Cyan)
    .bg(Color::DarkGray)
    .italic()
    .bold()
}

macro_rules! help_item {
  ($key: expr, $desc: expr) => {{
    [
      crate::tui::ui::help_key($key),
      crate::tui::ui::help_desc($desc),
      "\u{200b}".into(),
    ]
  }};
}

pub(crate) use help_item;
