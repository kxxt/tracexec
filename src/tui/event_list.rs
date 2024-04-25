// Copyright (c) 2023 Ratatui Developers
// Copyright (c) 2024 Levi Zim

// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
// associated documentation files (the "Software"), to deal in the Software without restriction,
// including without limitation the rights to use, copy, modify, merge, publish, distribute,
// sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all copies or substantial
// portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
// NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
// OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use ratatui::widgets::ListState;

use crate::event::TracerEvent;

pub struct EventList {
  pub state: ListState,
  pub items: Vec<TracerEvent>,
  /// Current window of the event list, [start, end)
  pub window: (usize, usize),
  /// How many items are there in the window
  pub nr_items_in_window: usize,
  last_selected: Option<usize>,
  pub horizontal_offset: usize,
  pub max_width: usize,
}

impl EventList {
  pub fn new() -> Self {
    Self {
      state: ListState::default(),
      items: vec![],
      last_selected: None,
      window: (0, 0),
      nr_items_in_window: 0,
      horizontal_offset: 0,
      max_width: 0,
    }
  }

  /// Try to slide down the window by one item
  /// Returns true if the window was slid down, false otherwise
  pub fn next_window(&mut self) -> bool {
    if self.window.1 < self.items.len() {
      self.window.0 += 1;
      self.window.1 += 1;
      true
    } else {
      false
    }
  }

  pub fn previous_window(&mut self) -> bool {
    if self.window.0 > 0 {
      self.window.0 -= 1;
      self.window.1 -= 1;
      true
    } else {
      false
    }
  }

  pub fn next(&mut self) {
    // i is the number of the selected item relative to the window
    let i = match self.state.selected() {
      Some(i) => if i >= self.window.1 - self.window.0 - 1 {
        self.next_window();
        i
      } else {
        i + 1
      }
      .min(self.nr_items_in_window - 1),
      None => self.last_selected.unwrap_or(0),
    };
    self.state.select(Some(i));
  }

  pub fn previous(&mut self) {
    let i = match self.state.selected() {
      Some(i) => {
        if i == 0 {
          self.previous_window();
          i
        } else {
          i - 1
        }
      }
      None => self.last_selected.unwrap_or(0),
    };
    self.state.select(Some(i));
  }

  pub fn unselect(&mut self) {
    let offset = self.state.offset();
    self.last_selected = self.state.selected();
    self.state.select(None);
    *self.state.offset_mut() = offset;
  }

  pub fn scroll_left(&mut self) {
    self.horizontal_offset = self.horizontal_offset.saturating_sub(1);
  }

  pub fn scroll_right(&mut self) {
    self.horizontal_offset = (self.horizontal_offset + 1).min(self.max_width.saturating_sub(1));
  }

  // TODO: this is ugly due to borrow checking.
  pub fn window(items: &[TracerEvent], window: (usize, usize)) -> &[TracerEvent] {
    &items[window.0..window.1.min(items.len())]
  }
}
