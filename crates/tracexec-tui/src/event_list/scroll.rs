use tracexec_core::event::EventId;

use super::EventList;

/// Vertical step by step scrolling
impl EventList {
  /// Try to slide down the window by one item
  /// Returns true if the window was slid down, false otherwise
  pub fn next_window(&mut self) -> bool {
    if self.events.is_empty() {
      return false;
    }
    if self.window.1 < self.events.len() {
      self.window.0 += 1;
      self.window.1 += 1;
      self.should_refresh_list_cache = true;
      true
    } else {
      false
    }
  }

  pub fn previous_window(&mut self) -> bool {
    if self.window.0 > 0 {
      self.window.0 -= 1;
      self.window.1 -= 1;
      self.should_refresh_list_cache = true;
      true
    } else {
      false
    }
  }

  pub fn next(&mut self) {
    // i is the number of the selected item relative to the window
    let i = match self.state.selected() {
      Some(i) => Some(
        if i >= self.window.1 - self.window.0 - 1 {
          self.next_window();
          i
        } else {
          i + 1
        }
        .min(self.nr_items_in_window.saturating_sub(1)),
      ),
      None => {
        if !self.events.is_empty() {
          Some(0)
        } else {
          None
        }
      }
    };
    self.state.select(i);
  }

  pub fn previous(&mut self) {
    let i = match self.state.selected() {
      Some(i) => Some(if i == 0 {
        self.previous_window();
        i
      } else {
        i - 1
      }),
      None => {
        if !self.events.is_empty() {
          Some(0)
        } else {
          None
        }
      }
    };
    self.state.select(i);
  }
}

/// Horizontal step by step scrolling
impl EventList {
  pub fn scroll_left(&mut self) {
    if self.horizontal_offset > 0 {
      self.horizontal_offset = self.horizontal_offset.saturating_sub(1);
      self.should_refresh_list_cache = true;
      tracing::trace!(
        "scroll_left: should_refresh_list_cache = {}",
        self.should_refresh_list_cache
      );
    }
  }

  pub fn scroll_right(&mut self) {
    let new_offset =
      (self.horizontal_offset + 1).min(self.max_width.saturating_sub(self.inner_width as usize));
    if new_offset != self.horizontal_offset {
      self.horizontal_offset = new_offset;
      self.should_refresh_list_cache = true;
      tracing::trace!(
        "scroll_right: should_refresh_list_cache = {}",
        self.should_refresh_list_cache
      );
    }
  }
}

/// Page level scrolling
impl EventList {
  pub fn page_down(&mut self) {
    if self.window.1 + self.max_window_len <= self.events.len() {
      self.window.0 += self.max_window_len;
      self.window.1 += self.max_window_len;
      self.should_refresh_list_cache = true;
    } else {
      // If we can't slide down the window by the number of items in the window
      // just set the window to the last items
      let old_window = self.window;
      self.window.0 = self.events.len().saturating_sub(self.max_window_len);
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_list_cache = old_window != self.window;
    }
    self.state.select(self.last_item_in_window_relative());
  }

  pub fn page_up(&mut self) {
    // Try to slide up the window by the number of items in the window
    if self.window.0 >= self.max_window_len {
      self.window.0 -= self.max_window_len;
      self.window.1 -= self.max_window_len;
      self.should_refresh_list_cache = true;
    } else {
      // If we can't slide up the window by the number of items in the window
      // just set the window to the first items
      let old_window = self.window;
      self.window.0 = 0;
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_list_cache = old_window != self.window;
    }
    self.select_first();
  }

  pub fn page_left(&mut self) {
    let old_offset = self.horizontal_offset;
    self.horizontal_offset = self
      .horizontal_offset
      .saturating_sub(self.inner_width as usize);
    if self.horizontal_offset != old_offset {
      self.should_refresh_list_cache = true;
    }
  }

  pub fn page_right(&mut self) {
    let old_offset = self.horizontal_offset;
    self.horizontal_offset = (self.horizontal_offset + self.inner_width as usize)
      .min(self.max_width.saturating_sub(self.inner_width as usize));
    if self.horizontal_offset != old_offset {
      self.should_refresh_list_cache = true;
    }
  }
}

/// Scroll to top/bottom/start/end/window top/window bottom
impl EventList {
  fn select_last(&mut self) {
    if !self.events.is_empty() {
      self.state.select(self.last_item_in_window_relative());
    }
  }

  fn select_first(&mut self) {
    if !self.events.is_empty() {
      self.state.select(Some(0));
    }
  }

  pub fn scroll_to_top(&mut self) {
    let old_window = self.window;
    self.window.0 = 0;
    self.window.1 = self.max_window_len;
    self.should_refresh_list_cache = old_window != self.window;
    self.select_first();
  }

  pub fn scroll_to_bottom(&mut self) {
    if self.events.is_empty() {
      return;
    }
    let old_window = self.window;
    self.window.0 = self.events.len().saturating_sub(self.max_window_len);
    self.window.1 = self.window.0 + self.max_window_len;
    self.select_last();
    self.should_refresh_list_cache = old_window != self.window;
  }

  pub fn scroll_to_start(&mut self) {
    if self.horizontal_offset > 0 {
      self.horizontal_offset = 0;
      self.should_refresh_list_cache = true;
      tracing::trace!(
        "scroll_to_start: should_refresh_list_cache = {}",
        self.should_refresh_list_cache
      );
    }
  }

  pub fn scroll_to_end(&mut self) {
    let new_offset = self.max_width.saturating_sub(self.inner_width as usize);
    if self.horizontal_offset < new_offset {
      self.horizontal_offset = new_offset;
      self.should_refresh_list_cache = true;
      tracing::trace!(
        "scroll_to_end: should_refresh_list_cache = {}",
        self.should_refresh_list_cache
      );
    }
  }
}

/// Scroll to id/index
impl EventList {
  pub fn scroll_to_id(&mut self, id: Option<EventId>) {
    let Some(id) = id else {
      return;
    };
    // self.window.0 should be <= its id
    self.scroll_to(Some((id - self.id_index_offset()).into_inner() as usize));
  }

  /// Scroll to the given index and select it,
  /// Usually the item will be at the top of the window,
  /// but if there are not enough items or the item is already in current window,
  /// no scrolling will be done,
  /// And if the item is in the last window, we won't scroll past it.
  fn scroll_to(&mut self, index: Option<usize>) {
    let Some(index) = index else {
      return;
    };
    if index < self.window.0 {
      // Scroll up
      self.window.0 = index;
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_list_cache = true;
      self.state.select(Some(0));
    } else if index >= self.window.1 {
      // Scroll down
      self.window.0 = index.min(self.events.len().saturating_sub(self.max_window_len));
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_list_cache = true;
      self.state.select(Some(index - self.window.0));
    } else {
      self.state.select(Some(index - self.window.0));
    }
  }
}
