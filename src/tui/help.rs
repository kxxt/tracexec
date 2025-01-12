use ratatui::{
  layout::Rect,
  style::Stylize,
  text::{Line, Text},
  widgets::{Paragraph, Wrap},
};

use crate::event::EventStatus;

use super::{sized_paragraph::SizedParagraph, theme::THEME};

use std::borrow::Cow;

use ratatui::{style::Styled, text::Span};

pub fn cli_flag<'a, T>(f: T) -> Span<'a>
where
  T: Into<Cow<'a, str>> + Styled<Item = Span<'a>>,
{
  f.set_style(THEME.cli_flag)
}

pub fn help_key<'a, T>(k: T) -> Span<'a>
where
  T: Into<Cow<'a, str>> + Styled<Item = Span<'a>>,
{
  let mut key_string = String::from("\u{00a0}");
  key_string.push_str(&k.into());
  key_string.push('\u{00a0}');
  key_string.set_style(THEME.help_key)
}
pub fn help_desc<'a, T>(d: T) -> Span<'a>
where
  T: Into<Cow<'a, str>> + Styled<Item = Span<'a>>,
{
  let mut desc_string = String::from("\u{00a0}");
  desc_string.push_str(&d.into());
  desc_string.push('\u{00a0}');
  desc_string.set_style(THEME.help_desc)
}

pub fn fancy_help_desc<'a, T>(d: T) -> Span<'a>
where
  T: Into<Cow<'a, str>> + Styled<Item = Span<'a>>,
{
  let mut desc_string = String::from("\u{00a0}");
  desc_string.push_str(&d.into());
  desc_string.push('\u{00a0}');
  desc_string.set_style(THEME.fancy_help_desc)
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
      "W".bold().black(),
      "elcome to tracexec! The TUI consists of at most two panes: the event list and optionally the pseudo terminal if ".into(),
      cli_flag("--tty/-t"),
      " is enabled. The event list displays the events emitted by the tracer. \
       The active pane's border is highlighted in cyan. \
       To switch active pane, press ".into(),
      help_key("Ctrl+S"),
      ". To send ".into(),
      help_key("Ctrl+S"),
      " to the pseudo terminal, press ".into(),
      help_key("Alt+S"),
      " when event list is active. The keybinding list at the bottom of the screen shows the available keys for currently active pane or popup.".into(),
    ]);
  let line2 = Line::default().spans(vec![
    "Y".bold().black(),
    "ou can navigate the event list using the arrow keys or ".into(),
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
    "W".bold(),
    "hen the pseudo terminal is active, you can interact with the terminal using the keyboard."
      .into(),
  ]);
  let line4 =
    Line::default().spans(vec![
    "E".bold().black(),
    "ach exec event in the event list consists of four parts, the pid, the status of the process,\
    the comm of the process (before exec), and the commandline to reproduce the exec event. \
    The pid is colored according to the result of the execve{,at} syscall.
    The status can be one of the following: "
      .into(),
    help_key(<&str>::from(EventStatus::ExecENOENT)),
    help_desc("Exec failed (ENOENT)"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ExecFailure)),
    help_desc("Exec failed"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessRunning)),
    help_desc("Running"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessPaused)),
    help_desc("Paused"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessDetached)),
    help_desc("Detached"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessExitedNormally)),
    help_desc("Exited normally"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessExitedAbnormally(1))),
    help_desc("Exited abnormally"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessTerminated)),
    help_desc("Terminated"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessKilled)),
    help_desc("Killed"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessSegfault)),
    help_desc("Segfault"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessInterrupted)),
    help_desc("Interrupted"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessIllegalInstruction)),
    help_desc("Illegal instruction"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessAborted)),
    help_desc("Aborted"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::ProcessSignaled(nix::sys::signal::Signal::SIGURG.into()))),
    help_desc("Signaled"),
    ", ".into(),
    help_key(<&str>::from(EventStatus::InternalError)),
    help_desc("An internal error occurred"),
  ]);

  let line5 = Line::default()
    .spans(vec![
      "P".bold().black(),
      "ress ".into(),
      help_key("Any Key"),
      " to close this help popup.".into(),
    ])
    .centered();
  let paragraph =
    Paragraph::new(Text::from_iter([line1, line2, line3, line4, line5])).wrap(Wrap { trim: false });
  let perhaps_a_suitable_width = area.width.saturating_sub(6) as usize;
  SizedParagraph::new(paragraph, perhaps_a_suitable_width)
}
