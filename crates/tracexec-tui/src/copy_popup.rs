use std::{
  cmp::min,
  sync::Arc,
};

use crossterm::event::KeyEvent;
use ratatui::{
  buffer::Buffer,
  layout::{
    Alignment,
    Rect,
  },
  style::{
    Color,
    Modifier,
    Style,
  },
  text::Span,
  widgets::{
    Block,
    Borders,
    Clear,
    HighlightSpacing,
    List,
    ListState,
    StatefulWidget,
    StatefulWidgetRef,
    Widget,
  },
};
use tracexec_core::{
  cli::keys::{
    KeyAction,
    KeyRouter,
    TuiKeyBindings,
  },
  event::TracerEventDetails,
};

use super::help::help_item;
use crate::{
  action::{
    Action,
    CopyTarget,
    SupportedShell::Bash,
  },
  theme::Theme,
};

#[derive(Debug, Clone)]
pub struct CopyPopup;

#[derive(Debug, Clone)]
pub struct CopyPopupState {
  pub event: Arc<TracerEventDetails>,
  pub state: ListState,
  pub available_targets: Vec<CopyTarget>,
  key_bindings: Arc<TuiKeyBindings>,
  theme: &'static Theme,
}

#[derive(Clone, Copy)]
struct CopyTargetConfig {
  target: CopyTarget,
  default_key: char,
  list_label: &'static str,
  help_label: &'static str,
}

const COPY_TARGETS: &[CopyTargetConfig] = &[
  CopyTargetConfig {
    target: CopyTarget::Commandline(Bash),
    default_key: 'c',
    list_label: "(C)ommand line",
    help_label: "Cmdline",
  },
  CopyTargetConfig {
    target: CopyTarget::CommandlineWithFullEnv(Bash),
    default_key: 'o',
    list_label: "C(o)mmand line with full env",
    help_label: "Cmdline with full env",
  },
  CopyTargetConfig {
    target: CopyTarget::CommandlineWithStdio(Bash),
    default_key: 's',
    list_label: "Command line with (S)tdio",
    help_label: "Cmdline with stdio",
  },
  CopyTargetConfig {
    target: CopyTarget::CommandlineWithFds(Bash),
    default_key: 'f',
    list_label: "Command line with (F)ile descriptors",
    help_label: "Cmdline with Fds",
  },
  CopyTargetConfig {
    target: CopyTarget::Env,
    default_key: 'e',
    list_label: "(E)nvironment variables",
    help_label: "Env",
  },
  CopyTargetConfig {
    target: CopyTarget::EnvDiff,
    default_key: 'd',
    list_label: "(D)iff of environment variables",
    help_label: "Diff of Env",
  },
  CopyTargetConfig {
    target: CopyTarget::Argv,
    default_key: 'a',
    list_label: "(A)rguments",
    help_label: "Argv",
  },
  CopyTargetConfig {
    target: CopyTarget::ArgvJoined,
    default_key: 'w',
    list_label: "Arguments joined by (W)hitespace",
    help_label: "Whitespace-joined argv",
  },
  CopyTargetConfig {
    target: CopyTarget::Filename,
    default_key: 'n',
    list_label: "File(N)ame",
    help_label: "Filename",
  },
  CopyTargetConfig {
    target: CopyTarget::SyscallResult,
    default_key: 'r',
    list_label: "Syscall (R)esult",
    help_label: "Result",
  },
  CopyTargetConfig {
    target: CopyTarget::Line,
    default_key: 'l',
    list_label: "Current (L)ine",
    help_label: "Line",
  },
];

impl CopyPopupState {
  pub fn new(
    event: Arc<TracerEventDetails>,
    key_bindings: Arc<TuiKeyBindings>,
    theme: &'static Theme,
  ) -> Self {
    let mut state = ListState::default();
    state.select(Some(0));
    let available_targets = if let TracerEventDetails::Exec(_) = &event.as_ref() {
      COPY_TARGETS.iter().map(|target| target.target).collect()
    } else {
      vec![CopyTarget::Line]
    };
    Self {
      event,
      state,
      available_targets,
      key_bindings,
      theme,
    }
  }

  fn selected_index(&self) -> usize {
    #[expect(clippy::unwrap_used)]
    // Copy popup must have a selected entry
    self.state.selected().unwrap()
  }

  pub fn next(&mut self) {
    self.state.select(Some(
      (self.selected_index() + 1).min(self.available_targets.len() - 1),
    ))
  }

  pub fn prev(&mut self) {
    self
      .state
      .select(Some(self.selected_index().saturating_sub(1)))
  }

  pub fn selected(&self) -> CopyTarget {
    let id = self.state.selected().unwrap_or(0);
    self.available_targets[id]
  }

  pub fn select_by_key(&mut self, key: KeyEvent) -> Option<CopyTarget> {
    let key = KeyRouter::new(&self.key_bindings, key);
    for (idx, target) in self.available_targets.iter().enumerate() {
      if key.matches(copy_target_action(*target)) {
        self.state.select(Some(idx));
        return Some(*target);
      }
    }
    None
  }

  pub fn help_items(&self) -> impl Iterator<Item = Span<'_>> {
    self.available_targets.iter().flat_map(|&target| {
      let config = copy_target_config(target);
      let key_label = self
        .key_bindings
        .bindings(copy_target_action(target))
        .display();
      help_item!(key_label, config.help_label, self.theme)
    })
  }

  fn list_label(&self, target: CopyTarget) -> String {
    let config = copy_target_config(target);
    let binding = self.key_bindings.bindings(copy_target_action(target));
    let uses_default_key = binding.0.len() == 1
      && binding
        .first()
        .and_then(|b| b.plain_char())
        .is_some_and(|ch| ch.eq_ignore_ascii_case(&config.default_key));
    if uses_default_key {
      config.list_label.to_string()
    } else {
      format!("{} ({})", config.help_label, binding.display())
    }
  }

  pub fn handle_key_event(&mut self, ke: KeyEvent) -> color_eyre::Result<Option<Action>> {
    let key = KeyRouter::new(&self.key_bindings, ke);
    if key.matches(KeyAction::ClosePopup) {
      return Ok(Some(Action::CancelCurrentPopup));
    }
    if key.matches(KeyAction::NextItem) {
      self.next();
      return Ok(None);
    }
    if key.matches(KeyAction::PrevItem) {
      self.prev();
      return Ok(None);
    }
    if key.matches(KeyAction::CopyChoose) {
      return Ok(Some(Action::CopyToClipboard {
        event: self.event.clone(),
        target: self.selected(),
      }));
    }
    if let Some(target) = self.select_by_key(ke) {
      return Ok(Some(Action::CopyToClipboard {
        event: self.event.clone(),
        target,
      }));
    }
    Ok(None)
  }
}

fn copy_target_config(target: CopyTarget) -> CopyTargetConfig {
  *COPY_TARGETS
    .iter()
    .find(|config| config.target == target)
    .expect("Missing copy target config")
}

fn copy_target_action(target: CopyTarget) -> KeyAction {
  match target {
    CopyTarget::Commandline(_) => KeyAction::CopyTargetCmdline,
    CopyTarget::CommandlineWithFullEnv(_) => KeyAction::CopyTargetCmdlineFullEnv,
    CopyTarget::CommandlineWithStdio(_) => KeyAction::CopyTargetCmdlineStdio,
    CopyTarget::CommandlineWithFds(_) => KeyAction::CopyTargetCmdlineFds,
    CopyTarget::Env => KeyAction::CopyTargetEnv,
    CopyTarget::EnvDiff => KeyAction::CopyTargetEnvDiff,
    CopyTarget::Argv => KeyAction::CopyTargetArgv,
    CopyTarget::ArgvJoined => KeyAction::CopyTargetArgvJoined,
    CopyTarget::Filename => KeyAction::CopyTargetFilename,
    CopyTarget::SyscallResult => KeyAction::CopyTargetSyscallResult,
    CopyTarget::Line => KeyAction::CopyTargetLine,
  }
}

impl StatefulWidgetRef for CopyPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut CopyPopupState) {
    let list = List::from_iter(
      state
        .available_targets
        .iter()
        .map(|&target| state.list_label(target)),
    )
    .block(
      Block::default()
        .title("Copy")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightGreen)),
    )
    .highlight_style(
      Style::default()
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::REVERSED)
        .fg(Color::Cyan),
    )
    .highlight_symbol(">")
    .highlight_spacing(HighlightSpacing::Always);
    let popup_area = centered_popup_rect(38, list.len() as u16, area);
    Clear.render(popup_area, buf);
    StatefulWidget::render(&list, popup_area, buf, &mut state.state);
  }

  type State = CopyPopupState;
}

// Copyright notice for the below code:

// MIT License

// Copyright (c) 2023 Josh McKinney

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

/// Create a rectangle centered in the given area.
fn centered_popup_rect(width: u16, height: u16, area: Rect) -> Rect {
  let height = height.saturating_add(2).min(area.height);
  let width = width.saturating_add(2).min(area.width);
  Rect {
    x: area.width.saturating_sub(width) / 2,
    y: area.height.saturating_sub(height) / 2,
    width: min(width, area.width),
    height: min(height, area.height),
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
  };
  use insta::assert_snapshot;
  use tracexec_core::{
    cli::keys::{
      KeyBinding,
      KeyList,
      TuiKeyBindings,
    },
    event::{
      TracerEventDetails,
      TracerEventMessage,
    },
  };

  use super::{
    COPY_TARGETS,
    CopyPopup,
    CopyPopupState,
  };
  use crate::{
    action::{
      Action,
      CopyTarget,
    },
    test_utils::{
      test_area_full,
      test_render_stateful_widget_area,
    },
    theme::current_theme,
  };

  fn info_popup(keys: TuiKeyBindings) -> CopyPopupState {
    CopyPopupState::new(
      Arc::new(TracerEventDetails::Info(TracerEventMessage {
        pid: None,
        timestamp: None,
        msg: "hello".to_string(),
      })),
      Arc::new(keys),
      current_theme(),
    )
  }

  #[test]
  fn copy_target_shortcuts_select_and_copy() -> color_eyre::Result<()> {
    let mut state = info_popup(TuiKeyBindings::default());
    state.available_targets = COPY_TARGETS.iter().map(|config| config.target).collect();
    for config in COPY_TARGETS {
      let action = state.handle_key_event(KeyEvent::new(
        KeyCode::Char(config.default_key),
        KeyModifiers::NONE,
      ))?;
      assert!(
        matches!(action, Some(Action::CopyToClipboard { target, .. }) if target == config.target)
      );
      assert_eq!(state.selected(), config.target);
    }
    Ok(())
  }

  #[test]
  fn copy_target_shortcuts_respect_remapping_and_availability() -> color_eyre::Result<()> {
    let keys = TuiKeyBindings {
      copy_target_line: KeyList(vec![KeyBinding::ctrl('x')]),
      copy_target_cmdline: KeyList(vec![KeyBinding::ctrl('x')]),
      ..Default::default()
    };
    let mut state = info_popup(keys);
    assert!(
      state
        .handle_key_event(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE))?
        .is_none()
    );
    assert_eq!(state.list_label(CopyTarget::Line), "Line (Ctrl+X)");
    assert!(matches!(
      state.handle_key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))?,
      Some(Action::CopyToClipboard {
        target: CopyTarget::Line,
        ..
      })
    ));

    state.key_bindings = Arc::new(TuiKeyBindings {
      copy_target_line: KeyList(vec![]),
      ..Default::default()
    });
    assert!(
      state
        .handle_key_event(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE))?
        .is_none()
    );
    assert!(
      state
        .handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))?
        .is_none()
    );
    Ok(())
  }

  #[test]
  fn popup_controls_take_priority_over_copy_shortcuts() -> color_eyre::Result<()> {
    let mut state = info_popup(TuiKeyBindings {
      copy_target_line: KeyList(vec![KeyBinding::char('q')]),
      ..Default::default()
    });
    assert!(matches!(
      state.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))?,
      Some(Action::CancelCurrentPopup)
    ));
    Ok(())
  }

  #[test]
  fn snapshot_copy_popup_info_event() {
    let mut state = info_popup(TuiKeyBindings::default());
    let area = test_area_full(40, 40);
    let rendered = test_render_stateful_widget_area(CopyPopup, area, &mut state);
    assert_snapshot!(rendered);
  }
}
