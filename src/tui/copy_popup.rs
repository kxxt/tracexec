use std::{cmp::min, collections::BTreeMap, sync::Arc};

use crossterm::event::{KeyCode, KeyEvent};
use lazy_static::lazy_static;
use ratatui::{
  buffer::Buffer,
  layout::{Alignment::Center, Rect},
  style::{Color, Modifier, Style},
  text::Span,
  widgets::{Block, Borders, Clear, HighlightSpacing, List, ListState, StatefulWidgetRef, Widget},
};

use crate::{
  action::{Action, CopyTarget, SupportedShell::Bash},
  event::TracerEvent,
};

use super::help::help_item;

#[derive(Debug, Clone)]
pub struct CopyPopup;

#[derive(Debug, Clone)]
pub struct CopyPopupState {
  pub event: Arc<TracerEvent>,
  pub state: ListState,
  pub available_targets: Vec<char>,
}

lazy_static! {
  pub static ref KEY_MAP: BTreeMap<char, (&'static str, &'static str)> = [
    ('c', ("(C)ommand line", "Cmdline")),
    ('s', ("Command line with (S)tdio", "Cmdline with stdio")),
    (
      'f',
      ("Command line with (F)ile descriptors", "Cmdline with Fds")
    ),
    ('e', ("(E)nvironment variables", "Env")),
    ('d', ("(D)iff of environment variables", "Diff of Env")),
    ('a', ("(A)rguments", "Argv")),
    ('n', ("File(N)ame", "Filename")),
    ('r', ("Syscall (R)esult", "Result")),
    ('l', ("Current (L)ine", "Line")),
  ]
  .into_iter()
  .collect();
}

impl CopyPopupState {
  pub fn new(event: Arc<TracerEvent>) -> Self {
    let mut state = ListState::default();
    state.select(Some(0));
    let available_targets = if let TracerEvent::Exec(_) = &event.as_ref() {
      KEY_MAP.keys().copied().collect()
    } else {
      vec!['l']
    };
    Self {
      event,
      state,
      available_targets,
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
    let key = self.available_targets[id];
    match key {
      'c' => CopyTarget::Commandline(Bash),
      's' => CopyTarget::CommandlineWithStdio(Bash),
      'f' => CopyTarget::CommandlineWithFds(Bash),
      'e' => CopyTarget::Env,
      'd' => CopyTarget::EnvDiff,
      'a' => CopyTarget::Argv,
      'n' => CopyTarget::Filename,
      'r' => CopyTarget::SyscallResult,
      'l' => CopyTarget::Line,
      _ => unreachable!(),
    }
  }

  pub fn select_by_key(&mut self, key: char) -> Option<CopyTarget> {
    if let Some(id) = self.available_targets.iter().position(|&k| k == key) {
      self.state.select(Some(id));
      Some(self.selected())
    } else {
      None
    }
  }

  pub fn help_items(&self) -> impl Iterator<Item = Span> {
    self.available_targets.iter().flat_map(|&key| {
      help_item!(
        key.to_ascii_uppercase().to_string(),
        KEY_MAP.get(&key).unwrap().1
      )
    })
  }

  pub fn handle_key_event(&mut self, ke: KeyEvent) -> color_eyre::Result<Option<Action>> {
    match ke.code {
      KeyCode::Char('q') => {
        return Ok(Some(Action::CancelCurrentPopup));
      }
      KeyCode::Down | KeyCode::Char('j') => {
        self.next();
      }
      KeyCode::Up | KeyCode::Char('k') => {
        self.prev();
      }
      KeyCode::Enter => {
        return Ok(Some(Action::CopyToClipboard {
          event: self.event.clone(),
          target: self.selected(),
        }));
      }
      KeyCode::Char(c) => {
        if let Some(target) = self.select_by_key(c) {
          return Ok(Some(Action::CopyToClipboard {
            event: self.event.clone(),
            target,
          }));
        }
      }
      _ => {}
    }
    Ok(None)
  }
}

impl StatefulWidgetRef for CopyPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut CopyPopupState) {
    let list = List::from_iter(
      state
        .available_targets
        .iter()
        .map(|&key| KEY_MAP.get(&key).unwrap().0),
    )
    .block(
      Block::default()
        .title("Copy")
        .title_alignment(Center)
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
    StatefulWidgetRef::render_ref(&list, popup_area, buf, &mut state.state);
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
