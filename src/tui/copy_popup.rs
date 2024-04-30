use std::{cmp::min, sync::Arc};

use ratatui::{
  buffer::Buffer,
  layout::{Alignment::Center, Rect},
  style::{Color, Modifier, Style},
  widgets::{Block, Borders, Clear, HighlightSpacing, List, ListState, StatefulWidgetRef, Widget},
};
use tui_popup::SizedWidgetRef;

use crate::{
  action::{CopyTarget, SupportedShell::Bash},
  event::TracerEvent,
};

#[derive(Debug, Clone)]
pub struct CopyPopup;

#[derive(Debug, Clone)]
pub struct CopyPopupState {
  pub event: Arc<TracerEvent>,
  pub state: ListState,
}

impl CopyPopupState {
  pub fn new(event: Arc<TracerEvent>) -> Self {
    let mut state = ListState::default();
    state.select(Some(0));
    Self { event, state }
  }

  pub fn next(&mut self) {
    self.state.select(Some(self.state.selected().unwrap() + 1))
  }

  pub fn prev(&mut self) {
    self
      .state
      .select(Some(self.state.selected().unwrap().saturating_sub(1)))
  }

  pub fn selected(&self) -> CopyTarget {
    let id = self.state.selected().unwrap_or(0);
    match id {
      0 => CopyTarget::Commandline(Bash),
      1 => CopyTarget::Env,
      2 => CopyTarget::EnvDiff,
      3 => CopyTarget::Argv,
      4 => CopyTarget::Filename,
      5 => CopyTarget::SyscallResult,
      _ => unreachable!(),
    }
  }

  pub fn select_by_key(&mut self, key: char) -> Option<CopyTarget> {
    let id = match key {
      'c' | 'C' => 0,
      'e' | 'E' => 1,
      'd' | 'D' => 2,
      'a' | 'A' => 3,
      'n' | 'N' => 4,
      's' | 'S' => 5,
      _ => return None,
    };
    self.state.select(Some(id));
    Some(self.selected())
  }
}

impl StatefulWidgetRef for CopyPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut CopyPopupState) {
    let list = List::new([
      "(C)ommand line",
      "(E)nvironment variables",
      "(D)iff of environment variables",
      "(A)rguments",
      "File(N)ame",
      "(S)yscall result",
    ])
    .block(
      Block::default()
        .title("Copy")
        .title_alignment(Center)
        .borders(Borders::ALL),
    )
    .highlight_style(
      Style::default()
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::REVERSED)
        .fg(Color::Cyan),
    )
    .highlight_symbol(">")
    .highlight_spacing(HighlightSpacing::Always);
    let popup_area = centered_popup_rect(35, 6, area);
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
  let height = height.saturating_add(2).try_into().unwrap_or(area.height);
  let width = width.saturating_add(2).try_into().unwrap_or(area.width);
  Rect {
    x: area.width.saturating_sub(width) / 2,
    y: area.height.saturating_sub(height) / 2,
    width: min(width, area.width),
    height: min(height, area.height),
  }
}
