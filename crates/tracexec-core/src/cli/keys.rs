use std::{
  fmt,
  str::FromStr,
};

use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use serde::{
  Deserialize,
  Deserializer,
  Serialize,
  Serializer,
  de,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
  pub code: KeyCode,
  pub modifiers: KeyModifiers,
}

impl KeyBinding {
  pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
    Self { code, modifiers }
  }

  pub const fn key(code: KeyCode) -> Self {
    Self::new(code, KeyModifiers::NONE)
  }

  pub const fn char(ch: char) -> Self {
    Self::new(KeyCode::Char(ch), KeyModifiers::NONE)
  }

  pub const fn ctrl(ch: char) -> Self {
    Self::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
  }

  pub const fn alt(ch: char) -> Self {
    Self::new(KeyCode::Char(ch), KeyModifiers::ALT)
  }

  pub fn matches(&self, key: KeyEvent) -> bool {
    self.code == key.code && self.modifiers == key.modifiers
  }

  pub fn display(&self) -> String {
    format_key_binding(self.code, self.modifiers)
  }

  pub fn display_without_modifiers(&self) -> String {
    format_key_code(self.code)
  }

  pub fn plain_char(&self) -> Option<char> {
    match (self.code, self.modifiers) {
      (KeyCode::Char(ch), KeyModifiers::NONE) => Some(ch),
      _ => None,
    }
  }
}

impl fmt::Display for KeyBinding {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.display())
  }
}

impl FromStr for KeyBinding {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    parse_key_binding(s)
  }
}

impl Serialize for KeyBinding {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.display())
  }
}

impl<'de> Deserialize<'de> for KeyBinding {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(de::Error::custom)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyList(pub Vec<KeyBinding>);

impl KeyList {
  pub fn matches(&self, key: KeyEvent) -> bool {
    self.0.iter().any(|binding| binding.matches(key))
  }

  pub fn display(&self) -> String {
    self.display_with_separator("/")
  }

  pub fn display_with_separator(&self, sep: &str) -> String {
    self
      .0
      .iter()
      .map(KeyBinding::display)
      .collect::<Vec<_>>()
      .join(sep)
  }

  pub fn first(&self) -> Option<&KeyBinding> {
    self.0.first()
  }
}

impl From<Vec<KeyBinding>> for KeyList {
  fn from(bindings: Vec<KeyBinding>) -> Self {
    Self(bindings)
  }
}

impl Serialize for KeyList {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    if self.0.len() == 1 {
      serializer.serialize_str(&self.0[0].display())
    } else {
      let values = self.0.iter().map(|b| b.display()).collect::<Vec<_>>();
      values.serialize(serializer)
    }
  }
}

impl<'de> Deserialize<'de> for KeyList {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    struct Visitor;

    impl<'de> de::Visitor<'de> for Visitor {
      type Value = KeyList;

      fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a key binding string or list of key binding strings")
      }

      fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
      where
        E: de::Error,
      {
        let binding: KeyBinding = v.parse().map_err(E::custom)?;
        Ok(KeyList(vec![binding]))
      }

      fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
      where
        A: de::SeqAccess<'de>,
      {
        let mut bindings = Vec::new();
        while let Some(value) = seq.next_element::<String>()? {
          bindings.push(value.parse().map_err(de::Error::custom)?);
        }
        Ok(KeyList(bindings))
      }
    }

    deserializer.deserialize_any(Visitor)
  }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TuiKeyBindingsConfig {
  pub quit: Option<KeyList>,
  pub switch_pane: Option<KeyList>,
  pub switch_layout: Option<KeyList>,
  pub close_popup: Option<KeyList>,
  pub help: Option<KeyList>,
  pub page_down: Option<KeyList>,
  pub page_up: Option<KeyList>,
  pub page_left: Option<KeyList>,
  pub page_right: Option<KeyList>,
  pub scroll_left: Option<KeyList>,
  pub scroll_right: Option<KeyList>,
  pub scroll_top: Option<KeyList>,
  pub scroll_bottom: Option<KeyList>,
  pub scroll_start: Option<KeyList>,
  pub scroll_end: Option<KeyList>,
  pub event_grow_pane: Option<KeyList>,
  pub event_shrink_pane: Option<KeyList>,
  pub event_send_ctrl_s: Option<KeyList>,
  pub event_toggle_follow: Option<KeyList>,
  pub event_search: Option<KeyList>,
  pub event_toggle_env: Option<KeyList>,
  pub event_toggle_cwd: Option<KeyList>,
  pub event_view_details: Option<KeyList>,
  pub event_go_to_parent: Option<KeyList>,
  pub event_backtrace: Option<KeyList>,
  pub event_copy: Option<KeyList>,
  pub event_breakpoints: Option<KeyList>,
  pub event_hits: Option<KeyList>,
  pub query_execute: Option<KeyList>,
  pub query_cancel: Option<KeyList>,
  pub query_toggle_case: Option<KeyList>,
  pub query_toggle_regex: Option<KeyList>,
  pub query_next_match: Option<KeyList>,
  pub query_prev_match: Option<KeyList>,
  pub query_clear: Option<KeyList>,
  pub details_scroll_down: Option<KeyList>,
  pub details_scroll_up: Option<KeyList>,
  pub details_next_tab: Option<KeyList>,
  pub details_prev_tab: Option<KeyList>,
  pub details_cycle_tab: Option<KeyList>,
  pub details_prev_field: Option<KeyList>,
  pub details_next_field: Option<KeyList>,
  pub details_copy: Option<KeyList>,
  pub details_view_parent: Option<KeyList>,
  pub next_item: Option<KeyList>,
  pub prev_item: Option<KeyList>,
  pub copy_choose: Option<KeyList>,
  pub copy_target_cmdline: Option<KeyList>,
  pub copy_target_cmdline_full_env: Option<KeyList>,
  pub copy_target_cmdline_stdio: Option<KeyList>,
  pub copy_target_cmdline_fds: Option<KeyList>,
  pub copy_target_env: Option<KeyList>,
  pub copy_target_env_diff: Option<KeyList>,
  pub copy_target_argv: Option<KeyList>,
  pub copy_target_filename: Option<KeyList>,
  pub copy_target_syscall_result: Option<KeyList>,
  pub copy_target_line: Option<KeyList>,
  pub go_back: Option<KeyList>,
  pub breakpoint_delete: Option<KeyList>,
  pub breakpoint_toggle_active: Option<KeyList>,
  pub breakpoint_edit: Option<KeyList>,
  pub breakpoint_new: Option<KeyList>,
  pub breakpoint_editor_save: Option<KeyList>,
  pub breakpoint_editor_cancel: Option<KeyList>,
  pub breakpoint_editor_toggle_stop: Option<KeyList>,
  pub breakpoint_editor_toggle_active: Option<KeyList>,
  pub hit_close: Option<KeyList>,
  pub hit_detach: Option<KeyList>,
  pub hit_resume: Option<KeyList>,
  pub hit_edit_default_command: Option<KeyList>,
  pub hit_run_default_command: Option<KeyList>,
  pub hit_run_custom_command: Option<KeyList>,
  pub hit_editor_save: Option<KeyList>,
  pub hit_editor_cancel: Option<KeyList>,
  pub hit_editor_clear: Option<KeyList>,
  pub terminal_toggle_scrollback: Option<KeyList>,
  pub terminal_scroll_up: Option<KeyList>,
  pub terminal_scroll_down: Option<KeyList>,
  pub terminal_page_up: Option<KeyList>,
  pub terminal_page_down: Option<KeyList>,
  pub terminal_scroll_top: Option<KeyList>,
  pub terminal_scroll_bottom: Option<KeyList>,
}

#[derive(Debug, Clone)]
pub struct TuiKeyBindings {
  pub quit: KeyList,
  pub switch_pane: KeyList,
  pub switch_layout: KeyList,
  pub close_popup: KeyList,
  pub help: KeyList,
  pub page_down: KeyList,
  pub page_up: KeyList,
  pub page_left: KeyList,
  pub page_right: KeyList,
  pub scroll_left: KeyList,
  pub scroll_right: KeyList,
  pub scroll_top: KeyList,
  pub scroll_bottom: KeyList,
  pub scroll_start: KeyList,
  pub scroll_end: KeyList,
  pub event_grow_pane: KeyList,
  pub event_shrink_pane: KeyList,
  pub event_send_ctrl_s: KeyList,
  pub event_toggle_follow: KeyList,
  pub event_search: KeyList,
  pub event_toggle_env: KeyList,
  pub event_toggle_cwd: KeyList,
  pub event_view_details: KeyList,
  pub event_go_to_parent: KeyList,
  pub event_backtrace: KeyList,
  pub event_copy: KeyList,
  pub event_breakpoints: KeyList,
  pub event_hits: KeyList,
  pub query_execute: KeyList,
  pub query_cancel: KeyList,
  pub query_toggle_case: KeyList,
  pub query_toggle_regex: KeyList,
  pub query_next_match: KeyList,
  pub query_prev_match: KeyList,
  pub query_clear: KeyList,
  pub details_scroll_down: KeyList,
  pub details_scroll_up: KeyList,
  pub details_next_tab: KeyList,
  pub details_prev_tab: KeyList,
  pub details_cycle_tab: KeyList,
  pub details_prev_field: KeyList,
  pub details_next_field: KeyList,
  pub details_copy: KeyList,
  pub details_view_parent: KeyList,
  pub next_item: KeyList,
  pub prev_item: KeyList,
  pub copy_choose: KeyList,
  pub copy_target_cmdline: KeyList,
  pub copy_target_cmdline_full_env: KeyList,
  pub copy_target_cmdline_stdio: KeyList,
  pub copy_target_cmdline_fds: KeyList,
  pub copy_target_env: KeyList,
  pub copy_target_env_diff: KeyList,
  pub copy_target_argv: KeyList,
  pub copy_target_filename: KeyList,
  pub copy_target_syscall_result: KeyList,
  pub copy_target_line: KeyList,
  pub go_back: KeyList,
  pub breakpoint_delete: KeyList,
  pub breakpoint_toggle_active: KeyList,
  pub breakpoint_edit: KeyList,
  pub breakpoint_new: KeyList,
  pub breakpoint_editor_save: KeyList,
  pub breakpoint_editor_cancel: KeyList,
  pub breakpoint_editor_toggle_stop: KeyList,
  pub breakpoint_editor_toggle_active: KeyList,
  pub hit_close: KeyList,
  pub hit_detach: KeyList,
  pub hit_resume: KeyList,
  pub hit_edit_default_command: KeyList,
  pub hit_run_default_command: KeyList,
  pub hit_run_custom_command: KeyList,
  pub hit_editor_save: KeyList,
  pub hit_editor_cancel: KeyList,
  pub hit_editor_clear: KeyList,
  pub terminal_toggle_scrollback: KeyList,
  pub terminal_scroll_up: KeyList,
  pub terminal_scroll_down: KeyList,
  pub terminal_page_up: KeyList,
  pub terminal_page_down: KeyList,
  pub terminal_scroll_top: KeyList,
  pub terminal_scroll_bottom: KeyList,
}

impl Default for TuiKeyBindings {
  fn default() -> Self {
    Self {
      quit: KeyList(vec![KeyBinding::char('q')]),
      switch_pane: KeyList(vec![KeyBinding::ctrl('s')]),
      switch_layout: KeyList(vec![KeyBinding::new(KeyCode::Char('l'), KeyModifiers::ALT)]),
      close_popup: KeyList(vec![KeyBinding::char('q')]),
      help: KeyList(vec![KeyBinding::key(KeyCode::F(1))]),
      page_down: KeyList(vec![
        KeyBinding::new(KeyCode::Down, KeyModifiers::CONTROL),
        KeyBinding::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        KeyBinding::key(KeyCode::PageDown),
      ]),
      page_up: KeyList(vec![
        KeyBinding::new(KeyCode::Up, KeyModifiers::CONTROL),
        KeyBinding::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        KeyBinding::key(KeyCode::PageUp),
      ]),
      page_left: KeyList(vec![
        KeyBinding::new(KeyCode::Left, KeyModifiers::CONTROL),
        KeyBinding::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
      ]),
      page_right: KeyList(vec![
        KeyBinding::new(KeyCode::Right, KeyModifiers::CONTROL),
        KeyBinding::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
      ]),
      scroll_left: KeyList(vec![KeyBinding::key(KeyCode::Left), KeyBinding::char('h')]),
      scroll_right: KeyList(vec![KeyBinding::key(KeyCode::Right), KeyBinding::char('l')]),
      scroll_top: KeyList(vec![KeyBinding::key(KeyCode::Home)]),
      scroll_bottom: KeyList(vec![KeyBinding::key(KeyCode::End)]),
      scroll_start: KeyList(vec![KeyBinding::new(KeyCode::Home, KeyModifiers::SHIFT)]),
      scroll_end: KeyList(vec![KeyBinding::new(KeyCode::End, KeyModifiers::SHIFT)]),
      event_grow_pane: KeyList(vec![KeyBinding::char('g')]),
      event_shrink_pane: KeyList(vec![KeyBinding::char('s')]),
      event_send_ctrl_s: KeyList(vec![KeyBinding::new(KeyCode::Char('s'), KeyModifiers::ALT)]),
      event_toggle_follow: KeyList(vec![KeyBinding::char('f')]),
      event_search: KeyList(vec![KeyBinding::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
      )]),
      event_toggle_env: KeyList(vec![KeyBinding::char('e')]),
      event_toggle_cwd: KeyList(vec![KeyBinding::char('w')]),
      event_view_details: KeyList(vec![KeyBinding::char('v')]),
      event_go_to_parent: KeyList(vec![KeyBinding::char('u')]),
      event_backtrace: KeyList(vec![KeyBinding::char('t')]),
      event_copy: KeyList(vec![KeyBinding::char('c')]),
      event_breakpoints: KeyList(vec![KeyBinding::char('b')]),
      event_hits: KeyList(vec![KeyBinding::char('z')]),
      query_execute: KeyList(vec![KeyBinding::key(KeyCode::Enter)]),
      query_cancel: KeyList(vec![KeyBinding::key(KeyCode::Esc)]),
      query_toggle_case: KeyList(vec![KeyBinding::new(KeyCode::Char('i'), KeyModifiers::ALT)]),
      query_toggle_regex: KeyList(vec![KeyBinding::new(KeyCode::Char('r'), KeyModifiers::ALT)]),
      query_next_match: KeyList(vec![KeyBinding::char('n')]),
      query_prev_match: KeyList(vec![KeyBinding::char('p')]),
      query_clear: KeyList(vec![KeyBinding::ctrl('u')]),
      details_scroll_down: KeyList(vec![KeyBinding::key(KeyCode::Down), KeyBinding::char('j')]),
      details_scroll_up: KeyList(vec![KeyBinding::key(KeyCode::Up), KeyBinding::char('k')]),
      details_next_tab: KeyList(vec![KeyBinding::key(KeyCode::Right), KeyBinding::char('l')]),
      details_prev_tab: KeyList(vec![KeyBinding::key(KeyCode::Left), KeyBinding::char('h')]),
      details_cycle_tab: KeyList(vec![KeyBinding::key(KeyCode::Tab)]),
      details_prev_field: KeyList(vec![KeyBinding::char('w')]),
      details_next_field: KeyList(vec![KeyBinding::char('s')]),
      details_copy: KeyList(vec![KeyBinding::char('c')]),
      details_view_parent: KeyList(vec![KeyBinding::char('u')]),
      next_item: KeyList(vec![KeyBinding::key(KeyCode::Down), KeyBinding::char('j')]),
      prev_item: KeyList(vec![KeyBinding::key(KeyCode::Up), KeyBinding::char('k')]),
      copy_choose: KeyList(vec![KeyBinding::key(KeyCode::Enter)]),
      copy_target_cmdline: KeyList(vec![KeyBinding::char('c')]),
      copy_target_cmdline_full_env: KeyList(vec![KeyBinding::char('o')]),
      copy_target_cmdline_stdio: KeyList(vec![KeyBinding::char('s')]),
      copy_target_cmdline_fds: KeyList(vec![KeyBinding::char('f')]),
      copy_target_env: KeyList(vec![KeyBinding::char('e')]),
      copy_target_env_diff: KeyList(vec![KeyBinding::char('d')]),
      copy_target_argv: KeyList(vec![KeyBinding::char('a')]),
      copy_target_filename: KeyList(vec![KeyBinding::char('n')]),
      copy_target_syscall_result: KeyList(vec![KeyBinding::char('r')]),
      copy_target_line: KeyList(vec![KeyBinding::char('l')]),
      go_back: KeyList(vec![KeyBinding::char('q')]),
      breakpoint_delete: KeyList(vec![
        KeyBinding::key(KeyCode::Delete),
        KeyBinding::char('d'),
      ]),
      breakpoint_toggle_active: KeyList(vec![KeyBinding::char(' ')]),
      breakpoint_edit: KeyList(vec![KeyBinding::key(KeyCode::Enter), KeyBinding::char('e')]),
      breakpoint_new: KeyList(vec![KeyBinding::char('n')]),
      breakpoint_editor_save: KeyList(vec![KeyBinding::key(KeyCode::Enter)]),
      breakpoint_editor_cancel: KeyList(vec![KeyBinding::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
      )]),
      breakpoint_editor_toggle_stop: KeyList(vec![KeyBinding::new(
        KeyCode::Char('s'),
        KeyModifiers::ALT,
      )]),
      breakpoint_editor_toggle_active: KeyList(vec![KeyBinding::new(
        KeyCode::Char('a'),
        KeyModifiers::ALT,
      )]),
      hit_close: KeyList(vec![KeyBinding::char('q')]),
      hit_detach: KeyList(vec![KeyBinding::char('d')]),
      hit_resume: KeyList(vec![KeyBinding::char('r')]),
      hit_edit_default_command: KeyList(vec![KeyBinding::char('e')]),
      hit_run_default_command: KeyList(vec![KeyBinding::key(KeyCode::Enter)]),
      hit_run_custom_command: KeyList(vec![KeyBinding::new(KeyCode::Enter, KeyModifiers::ALT)]),
      hit_editor_save: KeyList(vec![KeyBinding::key(KeyCode::Enter)]),
      hit_editor_cancel: KeyList(vec![
        KeyBinding::key(KeyCode::Esc),
        KeyBinding::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
      ]),
      hit_editor_clear: KeyList(vec![KeyBinding::ctrl('u')]),
      terminal_toggle_scrollback: KeyList(vec![KeyBinding::ctrl('u')]),
      terminal_scroll_up: KeyList(vec![KeyBinding::key(KeyCode::Up)]),
      terminal_scroll_down: KeyList(vec![KeyBinding::key(KeyCode::Down)]),
      terminal_page_up: KeyList(vec![KeyBinding::key(KeyCode::PageUp)]),
      terminal_page_down: KeyList(vec![KeyBinding::key(KeyCode::PageDown)]),
      terminal_scroll_top: KeyList(vec![KeyBinding::key(KeyCode::Home)]),
      terminal_scroll_bottom: KeyList(vec![KeyBinding::key(KeyCode::End)]),
    }
  }
}

impl TuiKeyBindings {
  pub fn from_config(config: Option<Box<TuiKeyBindingsConfig>>) -> Self {
    let mut keys = Self::default();
    if let Some(config) = config {
      keys.apply_config(config);
    }
    keys
  }

  pub fn apply_config(&mut self, config: Box<TuiKeyBindingsConfig>) {
    macro_rules! apply {
      ($($field:ident),+ $(,)?) => {
        $(
          if let Some(value) = config.$field {
            self.$field = value;
          }
        )+
      };
    }

    apply!(
      quit,
      switch_pane,
      switch_layout,
      close_popup,
      help,
      page_down,
      page_up,
      page_left,
      page_right,
      scroll_left,
      scroll_right,
      scroll_top,
      scroll_bottom,
      scroll_start,
      scroll_end,
      event_grow_pane,
      event_shrink_pane,
      event_send_ctrl_s,
      event_toggle_follow,
      event_search,
      event_toggle_env,
      event_toggle_cwd,
      event_view_details,
      event_go_to_parent,
      event_backtrace,
      event_copy,
      event_breakpoints,
      event_hits,
      query_execute,
      query_cancel,
      query_toggle_case,
      query_toggle_regex,
      query_next_match,
      query_prev_match,
      query_clear,
      details_scroll_down,
      details_scroll_up,
      details_next_tab,
      details_prev_tab,
      details_cycle_tab,
      details_prev_field,
      details_next_field,
      details_copy,
      details_view_parent,
      next_item,
      prev_item,
      copy_choose,
      copy_target_cmdline,
      copy_target_cmdline_full_env,
      copy_target_cmdline_stdio,
      copy_target_cmdline_fds,
      copy_target_env,
      copy_target_env_diff,
      copy_target_argv,
      copy_target_filename,
      copy_target_syscall_result,
      copy_target_line,
      go_back,
      breakpoint_delete,
      breakpoint_toggle_active,
      breakpoint_edit,
      breakpoint_new,
      breakpoint_editor_save,
      breakpoint_editor_cancel,
      breakpoint_editor_toggle_stop,
      breakpoint_editor_toggle_active,
      hit_close,
      hit_detach,
      hit_resume,
      hit_edit_default_command,
      hit_run_default_command,
      hit_run_custom_command,
      hit_editor_save,
      hit_editor_cancel,
      hit_editor_clear,
      terminal_toggle_scrollback,
      terminal_scroll_up,
      terminal_scroll_down,
      terminal_page_up,
      terminal_page_down,
      terminal_scroll_top,
      terminal_scroll_bottom,
    );
  }
}

fn parse_key_binding(input: &str) -> Result<KeyBinding, String> {
  let raw = input.trim();
  if raw.is_empty() {
    return Err("Key binding cannot be empty".into());
  }
  let mut modifiers = KeyModifiers::NONE;
  let mut key_part: Option<&str> = None;
  for part in raw.split('+') {
    let part = part.trim();
    if part.is_empty() {
      continue;
    }
    match part.to_ascii_lowercase().as_str() {
      "ctrl" | "control" | "ctl" => modifiers |= KeyModifiers::CONTROL,
      "alt" | "option" => modifiers |= KeyModifiers::ALT,
      "shift" => modifiers |= KeyModifiers::SHIFT,
      "super" | "meta" | "cmd" | "command" | "win" => modifiers |= KeyModifiers::SUPER,
      _ => {
        if key_part.is_some() {
          return Err(format!(
            "Invalid key binding: multiple key codes in \"{input}\""
          ));
        }
        key_part = Some(part);
      }
    }
  }
  let key_part = key_part.ok_or_else(|| format!("Invalid key binding \"{input}\""))?;
  if modifiers == KeyModifiers::NONE
    && key_part.len() == 1
    && key_part
      .chars()
      .next()
      .is_some_and(|ch| ch.is_ascii_uppercase())
  {
    modifiers |= KeyModifiers::SHIFT;
  }
  let code = parse_key_code(key_part, modifiers)?;

  Ok(KeyBinding::new(code, modifiers))
}

fn parse_key_code(input: &str, modifiers: KeyModifiers) -> Result<KeyCode, String> {
  let key = input.trim();
  if key.is_empty() {
    return Err("Key code cannot be empty".into());
  }
  let key_lower = key.to_ascii_lowercase();
  let code = match key_lower.as_str() {
    "enter" | "return" => KeyCode::Enter,
    "esc" | "escape" => KeyCode::Esc,
    "tab" => {
      if modifiers.contains(KeyModifiers::SHIFT) {
        KeyCode::BackTab
      } else {
        KeyCode::Tab
      }
    }
    "backtab" | "back_tab" | "back-tab" => KeyCode::BackTab,
    "backspace" | "bs" => KeyCode::Backspace,
    "delete" | "del" => KeyCode::Delete,
    "insert" | "ins" => KeyCode::Insert,
    "home" => KeyCode::Home,
    "end" => KeyCode::End,
    "pageup" | "pgup" | "pg_up" | "page_up" => KeyCode::PageUp,
    "pagedown" | "pgdn" | "pg_down" | "page_down" => KeyCode::PageDown,
    "up" => KeyCode::Up,
    "down" => KeyCode::Down,
    "left" => KeyCode::Left,
    "right" => KeyCode::Right,
    "space" | "spacebar" => KeyCode::Char(' '),
    _ if key.len() == 2 && key_lower.starts_with('f') => {
      let n = key_lower[1..]
        .parse::<u8>()
        .map_err(|_| format!("Invalid function key \"{input}\". Use F1..F12."))?;
      if (1..=12).contains(&n) {
        KeyCode::F(n)
      } else {
        return Err(format!("Function key out of range in \"{input}\""));
      }
    }
    _ if key.len() == 3 && key_lower.starts_with('f') => {
      let n = key_lower[1..]
        .parse::<u8>()
        .map_err(|_| format!("Invalid function key \"{input}\". Use F1..F12."))?;
      if (1..=12).contains(&n) {
        KeyCode::F(n)
      } else {
        return Err(format!("Function key out of range in \"{input}\""));
      }
    }
    _ => {
      let mut chars = key.chars();
      let ch = chars.next().ok_or_else(|| "Missing key code".to_string())?;
      if chars.next().is_some() {
        return Err(format!("Unknown key name \"{input}\""));
      }
      let ch = if ch.is_ascii_alphabetic() {
        if modifiers.contains(KeyModifiers::SHIFT) {
          ch.to_ascii_uppercase()
        } else {
          ch.to_ascii_lowercase()
        }
      } else {
        ch
      };
      KeyCode::Char(ch)
    }
  };
  Ok(code)
}

fn format_key_binding(code: KeyCode, modifiers: KeyModifiers) -> String {
  let mut parts = Vec::new();
  if modifiers.contains(KeyModifiers::CONTROL) {
    parts.push("Ctrl".to_string());
  }
  if modifiers.contains(KeyModifiers::ALT) {
    parts.push("Alt".to_string());
  }
  let show_shift = modifiers.contains(KeyModifiers::SHIFT)
    && !matches!(code, KeyCode::BackTab)
    && !(matches!(code, KeyCode::Char(ch) if ch.is_ascii_uppercase())
      && modifiers == KeyModifiers::SHIFT);
  if show_shift {
    parts.push("Shift".to_string());
  }
  if modifiers.contains(KeyModifiers::SUPER) {
    parts.push("Super".to_string());
  }
  let key = format_key_code(code);
  if parts.is_empty() {
    key
  } else {
    parts.push(key);
    parts.join("+")
  }
}

fn format_key_code(code: KeyCode) -> String {
  match code {
    KeyCode::Enter => "Enter".to_string(),
    KeyCode::Esc => "Esc".to_string(),
    KeyCode::Tab => "Tab".to_string(),
    KeyCode::BackTab => "Shift+Tab".to_string(),
    KeyCode::Backspace => "Backspace".to_string(),
    KeyCode::Delete => "Del".to_string(),
    KeyCode::Insert => "Ins".to_string(),
    KeyCode::Home => "Home".to_string(),
    KeyCode::End => "End".to_string(),
    KeyCode::PageUp => "PgUp".to_string(),
    KeyCode::PageDown => "PgDn".to_string(),
    KeyCode::Up => "↑".to_string(),
    KeyCode::Down => "↓".to_string(),
    KeyCode::Left => "←".to_string(),
    KeyCode::Right => "→".to_string(),
    KeyCode::Char(' ') => "Space".to_string(),
    KeyCode::Char(ch) => {
      if ch.is_ascii_alphabetic() {
        ch.to_ascii_uppercase().to_string()
      } else {
        ch.to_string()
      }
    }
    KeyCode::F(n) => format!("F{n}"),
    _ => format!("{code:?}"),
  }
}

#[cfg(test)]
mod tests {
  use toml;

  use super::*;

  #[test]
  fn test_parse_key_binding_ctrl_plus() {
    let binding: KeyBinding = "Ctrl + S".parse().unwrap();
    assert_eq!(binding.code, KeyCode::Char('s'));
    assert_eq!(binding.modifiers, KeyModifiers::CONTROL);
  }

  #[test]
  fn test_parse_key_binding_uppercase_without_shift() {
    let binding: KeyBinding = "Q".parse().unwrap();
    assert_eq!(binding.code, KeyCode::Char('Q'));
    assert_eq!(binding.modifiers, KeyModifiers::SHIFT);
  }

  #[test]
  fn test_parse_key_binding_shifted_letter() {
    let binding: KeyBinding = "Shift+q".parse().unwrap();
    assert_eq!(binding.code, KeyCode::Char('Q'));
    assert_eq!(binding.modifiers, KeyModifiers::SHIFT);
  }

  #[test]
  fn test_key_list_deserialize_single() {
    #[derive(Deserialize)]
    struct Wrapper {
      keys: KeyList,
    }
    let list = toml::from_str::<Wrapper>(r#"keys = "Ctrl+F""#)
      .unwrap()
      .keys;
    assert_eq!(list.0.len(), 1);
    assert_eq!(list.0[0].code, KeyCode::Char('f'));
  }

  #[test]
  fn test_key_list_deserialize_array() {
    #[derive(Deserialize)]
    struct Wrapper {
      keys: KeyList,
    }
    let list = toml::from_str::<Wrapper>(r#"keys = ["Down", "J"]"#)
      .unwrap()
      .keys;
    assert_eq!(list.0.len(), 2);
  }
}
