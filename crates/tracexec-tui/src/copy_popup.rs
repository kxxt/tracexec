use std::{
  cmp::min,
  sync::Arc,
};

use crossterm::event::{
  KeyEvent,
  MouseButton,
  MouseEvent,
  MouseEventKind,
};
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
  cli::keys::TuiKeyBindings,
  event::TracerEventDetails,
};

use super::help::{
  HelpItem,
  help_item,
};
use crate::{
  action::{
    Action,
    CopyTarget,
    SupportedShell::Bash,
  },
  mouse::position_in_rect,
  theme::Theme,
};

#[derive(Debug, Clone)]
pub struct CopyPopup;

#[derive(Debug, Clone)]
pub struct CopyPopupState {
  pub event: Arc<TracerEventDetails>,
  pub state: ListState,
  pub available_targets: Vec<CopyTarget>,
  rendered_area: Rect,
  list_area: Rect,
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
      rendered_area: Rect::default(),
      list_area: Rect::default(),
      key_bindings,
      theme,
    }
  }

  pub fn next(&mut self) {
    self.state.select(Some(
      (self.state.selected().unwrap() + 1).min(self.available_targets.len() - 1),
    ))
  }

  pub fn prev(&mut self) {
    self
      .state
      .select(Some(self.state.selected().unwrap().saturating_sub(1)))
  }

  pub fn selected(&self) -> CopyTarget {
    let id = self.state.selected().unwrap_or(0);
    self.available_targets[id]
  }

  pub fn select_by_key(&mut self, key: KeyEvent) -> Option<CopyTarget> {
    for (idx, target) in self.available_targets.iter().enumerate() {
      if copy_target_binding(&self.key_bindings, *target).matches(key) {
        self.state.select(Some(idx));
        return Some(*target);
      }
    }
    None
  }

  pub fn help_items(&self) -> impl Iterator<Item = HelpItem<'_>> {
    self.available_targets.iter().map(|&target| {
      let config = copy_target_config(target);
      let key_label = copy_target_binding(&self.key_bindings, target).display();
      help_item!(key_label, config.help_label, self.theme)
    })
  }

  fn list_label(&self, target: CopyTarget) -> String {
    let config = copy_target_config(target);
    let binding = copy_target_binding(&self.key_bindings, target);
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
    if self.key_bindings.close_popup.matches(ke) {
      return Ok(Some(Action::CancelCurrentPopup));
    }
    if self.key_bindings.next_item.matches(ke) {
      self.next();
      return Ok(None);
    }
    if self.key_bindings.prev_item.matches(ke) {
      self.prev();
      return Ok(None);
    }
    if self.key_bindings.copy_choose.matches(ke) {
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

  pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> Option<Action> {
    let col = event.column;
    let row = event.row;

    if !position_in_rect(col, row, &self.rendered_area) {
      return None;
    }

    let hovered_target = position_in_rect(col, row, &self.list_area)
      .then_some((row - self.list_area.y) as usize)
      .filter(|idx| *idx < self.available_targets.len());

    match event.kind {
      MouseEventKind::Down(MouseButton::Left) => {
        if let Some(idx) = hovered_target {
          self.state.select(Some(idx));
          return Some(Action::CopyToClipboard {
            event: self.event.clone(),
            target: self.selected(),
          });
        }
      }
      MouseEventKind::Moved => {
        if let Some(idx) = hovered_target {
          self.state.select(Some(idx));
        }
      }
      MouseEventKind::ScrollUp => self.prev(),
      MouseEventKind::ScrollDown => self.next(),
      _ => {}
    }

    None
  }
}

fn copy_target_config(target: CopyTarget) -> CopyTargetConfig {
  *COPY_TARGETS
    .iter()
    .find(|config| config.target == target)
    .expect("Missing copy target config")
}

fn copy_target_binding(
  keys: &TuiKeyBindings,
  target: CopyTarget,
) -> &tracexec_core::cli::keys::KeyList {
  match target {
    CopyTarget::Commandline(_) => &keys.copy_target_cmdline,
    CopyTarget::CommandlineWithFullEnv(_) => &keys.copy_target_cmdline_full_env,
    CopyTarget::CommandlineWithStdio(_) => &keys.copy_target_cmdline_stdio,
    CopyTarget::CommandlineWithFds(_) => &keys.copy_target_cmdline_fds,
    CopyTarget::Env => &keys.copy_target_env,
    CopyTarget::EnvDiff => &keys.copy_target_env_diff,
    CopyTarget::Argv => &keys.copy_target_argv,
    CopyTarget::Filename => &keys.copy_target_filename,
    CopyTarget::SyscallResult => &keys.copy_target_syscall_result,
    CopyTarget::Line => &keys.copy_target_line,
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
    state.rendered_area = popup_area;
    state.list_area = Rect {
      x: popup_area.x.saturating_add(1),
      y: popup_area.y.saturating_add(1),
      width: popup_area.width.saturating_sub(2),
      height: popup_area.height.saturating_sub(2),
    };
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
    x: area.x + area.width.saturating_sub(width) / 2,
    y: area.y + area.height.saturating_sub(height) / 2,
    width: min(width, area.width),
    height: min(height, area.height),
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    sync::Arc,
  };

  use crossterm::event::{
    KeyModifiers,
    MouseButton,
    MouseEvent,
    MouseEventKind,
  };
  use insta::assert_snapshot;
  use nix::unistd::Pid;
  use tracexec_core::{
    cache::ArcStr,
    cli::keys::TuiKeyBindings,
    event::{
      ExecEvent,
      ExecSyscall,
      OutputMsg,
      TracerEventDetails,
      TracerEventMessage,
    },
    proc::{
      CgroupInfo,
      FileDescriptorInfoCollection,
      diff_env,
    },
    timestamp::ts_from_boot_ns,
  };

  use super::{
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

  fn exec_details() -> TracerEventDetails {
    TracerEventDetails::Exec(Box::new(ExecEvent {
      syscall: ExecSyscall::Execve,
      exec_pid: Pid::from_raw(100),
      pid: Pid::from_raw(100),
      cwd: OutputMsg::Ok(ArcStr::from("/tmp")),
      comm: ArcStr::from("cmd"),
      filename: OutputMsg::Ok(ArcStr::from("/bin/true")),
      argv: Arc::new(Ok(vec![OutputMsg::Ok(ArcStr::from("true"))])),
      envp: Arc::new(Ok(BTreeMap::new())),
      has_dash_env: false,
      cred: Ok(Default::default()),
      interpreter: None,
      env_diff: Ok(diff_env(&BTreeMap::new(), &BTreeMap::new())),
      fdinfo: Arc::new(FileDescriptorInfoCollection::default()),
      result: 0,
      timestamp: ts_from_boot_ns(1),
      parent: None,
      cgroup: CgroupInfo::V2 {
        path: "/".to_string(),
      },
    }))
  }

  #[test]
  fn snapshot_copy_popup_info_event() {
    let event = Arc::new(TracerEventDetails::Info(TracerEventMessage {
      pid: None,
      timestamp: None,
      msg: "hello".to_string(),
    }));
    let mut state =
      CopyPopupState::new(event, Arc::new(TuiKeyBindings::default()), current_theme());
    let area = test_area_full(40, 40);
    let rendered = test_render_stateful_widget_area(CopyPopup, area, &mut state);
    assert_snapshot!(rendered);
  }

  #[test]
  fn mouse_click_on_target_selects_and_copies() {
    let event = Arc::new(TracerEventDetails::Info(TracerEventMessage {
      pid: None,
      timestamp: None,
      msg: "hello".to_string(),
    }));
    let mut state =
      CopyPopupState::new(event, Arc::new(TuiKeyBindings::default()), current_theme());
    let area = test_area_full(40, 40);
    let _ = test_render_stateful_widget_area(CopyPopup, area, &mut state);

    let action = state.handle_mouse_event(&MouseEvent {
      kind: MouseEventKind::Down(MouseButton::Left),
      column: state.list_area.x,
      row: state.list_area.y,
      modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(
      action,
      Some(Action::CopyToClipboard {
        target: CopyTarget::Line,
        ..
      })
    ));
  }

  #[test]
  fn mouse_move_over_target_updates_highlight() {
    let event = Arc::new(exec_details());
    let mut state =
      CopyPopupState::new(event, Arc::new(TuiKeyBindings::default()), current_theme());
    let area = test_area_full(80, 40);
    let _ = test_render_stateful_widget_area(CopyPopup, area, &mut state);

    let action = state.handle_mouse_event(&MouseEvent {
      kind: MouseEventKind::Moved,
      column: state.list_area.x,
      row: state.list_area.y + 2,
      modifiers: KeyModifiers::NONE,
    });

    assert!(action.is_none());
    assert_eq!(state.state.selected(), Some(2));
  }
}
