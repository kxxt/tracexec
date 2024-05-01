use ratatui::{
  layout::Rect,
  style::{Color, Stylize},
  text::{Line, Text},
  widgets::{Paragraph, Wrap},
};

use super::sized_paragraph::SizedParagraph;

use std::borrow::Cow;

use ratatui::{style::Styled, text::Span};

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
  key_string.push('\u{00a0}');
  key_string.fg(Color::Black).bg(Color::Cyan).bold()
}
pub fn help_desc<'a, T>(d: T) -> Span<'a>
where
  T: Into<Cow<'a, str>>,
  T: Styled<Item = Span<'a>>,
{
  let mut desc_string = String::from("\u{00a0}");
  desc_string.push_str(&d.into());
  desc_string.push('\u{00a0}');
  desc_string
    .fg(Color::LightGreen)
    .bg(Color::DarkGray)
    .italic()
    .bold()
}

macro_rules! help_item {
  ($key: expr, $desc: expr) => {{
    [
      crate::tui::help::help_key($key),
      crate::tui::help::help_desc($desc),
      "\u{200b}".into(),
    ]
  }};
}

pub(crate) use help_item;

pub fn help<'a>(area: Rect) -> SizedParagraph<'a> {
  let line1 = Line::default().spans(vec![
      "Welcome to tracexec! The TUI consists of at most two panes: the event list and optionally the pseudo terminal if ".into(),
      cli_flag("--tty/-t"),
      " is enabled. The event list displays the events emitted by the tracer. \
       The active pane's border is highlighted in cyan. \
       To switch active pane, press ".into(),
      help_key("Ctrl+S"),
      ". The keybinding list at the bottom of the screen shows the available keys for currently active pane or popup.".into(),
    ]);
  let line2 = Line::default().spans(vec![
    "You can navigate the event list using the arrow keys or ".into(),
    help_key("H/J/K/L"),
    ". To scroll faster, use ".into(),
    help_key("Ctrl+↑/↓/←/→/H/J/K/L"),
    " or ".into(),
    help_key("PgUp/PgDn"),
    ". Use ".into(),
    help_key("(Shift +) Home/End"),
    " to scroll to the (line start/line end)/top/bottom. Press ".into(),
    help_key("F"),
    " to toggle follow mode, which will keep the list scrolled to bottom. ".into(),
    "To change pane size, press ".into(),
    help_key("G/S"),
    " when the active pane is event list. ".into(),
    "To switch between horizontal and vertical layout, press ".into(),
    help_key("Alt+L"),
    ". To view the details of the selected event, press ".into(),
    help_key("V"),
    ". To copy the selected event to the clipboard, press ".into(),
    help_key("C"),
    " then select what to copy. To quit, press ".into(),
    help_key("Q"),
    " while the event list is active.".into(),
  ]);
  let line3 = Line::default().spans(vec![
    "When the pseudo terminal is active, you can interact with the terminal using the keyboard.",
  ]);
  let line4 = Line::default()
    .spans(vec![
      "Press ".into(),
      help_key("Any Key"),
      " to close this help popup.".into(),
    ])
    .centered();
  let paragraph =
    Paragraph::new(Text::from_iter([line1, line2, line3, line4])).wrap(Wrap { trim: false });
  let perhaps_a_suitable_width = area.width.saturating_sub(6) as usize;
  SizedParagraph::new(paragraph, perhaps_a_suitable_width)
}
