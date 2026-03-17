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

#[cfg(test)]
mod tests {
  use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
  };

  use tracexec_core::{
    cli::args::ModifierArgs,
    event::{
      EventId,
      OutputMsg,
      TracerEventDetails,
      TracerEventMessage,
    },
    proc::{
      BaselineInfo,
      FileDescriptorInfoCollection,
    },
  };

  use super::EventList;
  use crate::event_list::Event;

  fn baseline_for_tests() -> Arc<BaselineInfo> {
    Arc::new(BaselineInfo {
      cwd: OutputMsg::Ok("cwd".into()),
      env: BTreeMap::new(),
      fdinfo: FileDescriptorInfoCollection::default(),
    })
  }

  fn make_list(len: usize, window_len: usize) -> EventList {
    let mut list = EventList::new(
      baseline_for_tests(),
      false,
      ModifierArgs::default(),
      1024,
      false,
      false,
      true,
    );
    list.max_window_len = window_len.max(1);
    list.window = (0, window_len.min(len));
    list.nr_items_in_window = list.window.1.saturating_sub(list.window.0);
    list.inner_width = 20;
    list.max_width = 80;
    for i in 0..len {
      let details = Arc::new(TracerEventDetails::Info(TracerEventMessage {
        pid: None,
        timestamp: None,
        msg: format!("event {i}"),
      }));
      let event = Event {
        details,
        status: None,
        elapsed: None,
        id: EventId::new(i as u64),
      };
      list.events.push_back(Rc::new(RefCell::new(event)));
    }
    if len > 0 {
      list.state.select(Some(0));
    }
    list
  }

  #[test]
  fn next_and_previous_window_slide() {
    let mut list = make_list(5, 3);
    assert!(list.next_window());
    assert_eq!(list.window, (1, 4));
    assert!(list.previous_window());
    assert_eq!(list.window, (0, 3));
  }

  #[test]
  fn next_advances_selection_and_window() {
    let mut list = make_list(5, 3);
    list.state.select(Some(2));
    list.next();
    assert_eq!(list.window, (1, 4));
    assert_eq!(list.state.selected(), Some(2));
  }

  #[test]
  fn scroll_to_id_moves_window_and_selection() {
    let mut list = make_list(6, 3);
    list.scroll_to_id(Some(EventId::new(4)));
    assert_eq!(list.window, (3, 6));
    assert_eq!(list.state.selected(), Some(1));
  }

  #[test]
  fn horizontal_scroll_respects_bounds() {
    let mut list = make_list(1, 1);
    list.scroll_left();
    assert_eq!(list.horizontal_offset, 0);
    list.scroll_right();
    assert_eq!(list.horizontal_offset, 1);
    list.scroll_to_end();
    assert_eq!(list.horizontal_offset, 60);
    list.scroll_to_start();
    assert_eq!(list.horizontal_offset, 0);
  }
}
