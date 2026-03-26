use std::borrow::Cow;

use crossterm::event::{
  KeyCode,
  KeyModifiers,
};
use ratatui::{
  layout::Rect,
  style::{
    Styled,
    Stylize,
  },
  text::{
    Line,
    Span,
    Text,
  },
  widgets::{
    Paragraph,
    Wrap,
  },
};
use tracexec_core::{
  cli::keys::{
    KeyBinding,
    KeyList,
    TuiKeyBindings,
  },
  event::EventStatus,
};

use super::{
  sized_paragraph::SizedParagraph,
  theme::THEME,
};

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
      crate::help::help_key($key),
      crate::help::help_desc($desc),
      "\u{200b}".into(),
    ]
  }};
}

pub(crate) use help_item;

fn format_nav_keys(keys: &TuiKeyBindings) -> String {
  let mut parts = Vec::new();
  for list in [
    &keys.scroll_left,
    &keys.next_item,
    &keys.prev_item,
    &keys.scroll_right,
  ] {
    if let Some(ch) = list
      .0
      .iter()
      .find_map(|binding| binding.plain_char())
      .filter(|ch| ch.is_ascii_alphabetic())
    {
      parts.push(ch.to_ascii_uppercase().to_string());
    } else {
      return format!(
        "{}/{}/{}/{}",
        keys.scroll_left.display(),
        keys.next_item.display(),
        keys.prev_item.display(),
        keys.scroll_right.display()
      );
    }
  }
  parts.join("/")
}

fn list_has_code(list: &KeyList, code: KeyCode) -> bool {
  list
    .0
    .iter()
    .any(|binding| binding.code == code && binding.modifiers == KeyModifiers::NONE)
}

fn nav_has_arrows(keys: &TuiKeyBindings) -> bool {
  list_has_code(&keys.next_item, KeyCode::Down)
    && list_has_code(&keys.prev_item, KeyCode::Up)
    && list_has_code(&keys.scroll_left, KeyCode::Left)
    && list_has_code(&keys.scroll_right, KeyCode::Right)
}

fn ctrl_label_for_code(list: &KeyList, code: KeyCode) -> Option<String> {
  list.0.iter().find_map(|binding| {
    if binding.code == code && binding.modifiers == KeyModifiers::CONTROL {
      Some(binding.display_without_modifiers())
    } else {
      None
    }
  })
}

fn ctrl_label_for_char(list: &KeyList, ch: char) -> Option<String> {
  list
    .0
    .iter()
    .find_map(|binding| match (binding.code, binding.modifiers) {
      (KeyCode::Char(c), KeyModifiers::CONTROL) if c.eq_ignore_ascii_case(&ch) => {
        Some(binding.display_without_modifiers())
      }
      _ => None,
    })
}

fn format_fast_ctrl_keys(keys: &TuiKeyBindings) -> String {
  let labels = [
    ctrl_label_for_code(&keys.page_up, KeyCode::Up),
    ctrl_label_for_code(&keys.page_down, KeyCode::Down),
    ctrl_label_for_code(&keys.page_left, KeyCode::Left),
    ctrl_label_for_code(&keys.page_right, KeyCode::Right),
    ctrl_label_for_char(&keys.page_left, 'h'),
    ctrl_label_for_char(&keys.page_down, 'j'),
    ctrl_label_for_char(&keys.page_up, 'k'),
    ctrl_label_for_char(&keys.page_right, 'l'),
  ];
  if labels.iter().all(|item| item.is_some()) {
    let joined = labels
      .iter()
      .filter_map(|item| item.as_ref())
      .cloned()
      .collect::<Vec<_>>()
      .join("/");
    format!("Ctrl+{joined}")
  } else {
    format!(
      "{}/{}/{}/{}",
      keys.page_up.display(),
      keys.page_down.display(),
      keys.page_left.display(),
      keys.page_right.display()
    )
  }
}

fn label_for_code(list: &KeyList, code: KeyCode) -> Option<String> {
  list.0.iter().find_map(|binding| {
    if binding.code == code && binding.modifiers == KeyModifiers::NONE {
      Some(binding.display_without_modifiers())
    } else {
      None
    }
  })
}

fn format_page_keys(keys: &TuiKeyBindings) -> String {
  let up = label_for_code(&keys.page_up, KeyCode::PageUp);
  let down = label_for_code(&keys.page_down, KeyCode::PageDown);
  if let (Some(up), Some(down)) = (up, down) {
    format!("{}/{}", up, down)
  } else {
    format!("{}/{}", keys.page_up.display(), keys.page_down.display())
  }
}

fn format_jump_keys(keys: &TuiKeyBindings) -> String {
  let home = label_for_code(&keys.scroll_top, KeyCode::Home);
  let end = label_for_code(&keys.scroll_bottom, KeyCode::End);
  let shift_home = keys
    .scroll_start
    .0
    .iter()
    .any(|binding| binding.code == KeyCode::Home && binding.modifiers == KeyModifiers::SHIFT);
  let shift_end = keys
    .scroll_end
    .0
    .iter()
    .any(|binding| binding.code == KeyCode::End && binding.modifiers == KeyModifiers::SHIFT);
  if home.is_some() && end.is_some() && shift_home && shift_end {
    "(Shift +) Home/End".to_string()
  } else {
    format!(
      "{}/{}/{}/{}",
      keys.scroll_top.display(),
      keys.scroll_bottom.display(),
      keys.scroll_start.display(),
      keys.scroll_end.display()
    )
  }
}

pub fn help<'a>(area: Rect, keys: &TuiKeyBindings) -> SizedParagraph<'a> {
  let vim_nav_keys = format_nav_keys(keys);
  let fast_ctrl_keys = format_fast_ctrl_keys(keys);
  let page_keys = format_page_keys(keys);
  let jump_keys = format_jump_keys(keys);
  let line1 = Line::default().spans(vec![
      "W".bold().black(),
      "elcome to tracexec! The TUI consists of at most two panes: the event list and optionally the pseudo terminal if ".into(),
      cli_flag("--tty/-t"),
      " is enabled. The event list displays the events emitted by the tracer. \
       The active pane's border is highlighted in cyan. \
       To switch active pane, press ".into(),
      help_key(keys.switch_pane.display()),
      ". To send ".into(),
      help_key(KeyBinding::ctrl('s').display()),
      " to the pseudo terminal, press ".into(),
      help_key(keys.event_send_ctrl_s.display()),
      " when event list is active. The keybinding list at the bottom of the screen shows the available keys for currently active pane or popup.".into(),
    ]);
  let line2 = Line::default().spans(vec![
    "Y".bold().black(),
    if nav_has_arrows(keys) {
      "ou can navigate the event list using the arrow keys or ".into()
    } else {
      "ou can navigate the event list using ".into()
    },
    help_key(vim_nav_keys),
    ". To scroll faster, use ".into(),
    help_key(fast_ctrl_keys),
    " or ".into(),
    help_key(page_keys),
    ". Use ".into(),
    help_key(jump_keys),
    " to scroll to the (line start/line end)/top/bottom. Press ".into(),
    help_key(keys.event_toggle_follow.display()),
    " to toggle follow mode, which will keep the list scrolled to bottom. ".into(),
    "To change pane size, press ".into(),
    help_key(format!(
      "{}/{}",
      keys.event_grow_pane.display(),
      keys.event_shrink_pane.display()
    )),
    " when the active pane is event list. ".into(),
    "To switch between horizontal and vertical layout, press ".into(),
    help_key(keys.switch_layout.display()),
    ". To view the details of the selected event, press ".into(),
    help_key(keys.event_view_details.display()),
    ". To copy the selected event to the clipboard, press ".into(),
    help_key(keys.event_copy.display()),
    " then select what to copy. To jump to the parent exec event of the currently selected event, press ".into(),
    help_key(keys.event_go_to_parent.display()),
    ". To show the backtrace of the currently selected event, press ".into(),
    help_key(keys.event_backtrace.display()),
    ". To quit, press ".into(),
    help_key(keys.quit.display()),
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

#[cfg(test)]
mod tests {
  use insta::assert_snapshot;

  use super::{
    TuiKeyBindings,
    help,
  };
  use crate::test_utils::{
    test_area_full,
    test_render_widget_area,
  };

  #[test]
  fn snapshot_help_popup() {
    let area = test_area_full(80, 40);
    let keys = TuiKeyBindings::default();
    let rendered = test_render_widget_area(help(area, &keys), area);
    assert_snapshot!(rendered);
  }
}
