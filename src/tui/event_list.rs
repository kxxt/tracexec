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

use std::{collections::VecDeque, sync::Arc};

use ratatui::{
  layout::Alignment::Right,
  prelude::{Buffer, Rect},
  style::{Color, Modifier, Style},
  text::Line,
  widgets::{
    block::Title, HighlightSpacing, List, ListItem, ListState, Scrollbar, ScrollbarOrientation,
    ScrollbarState, StatefulWidget, StatefulWidgetRef, Widget,
  },
};

use crate::{cli::args::ModifierArgs, event::TracerEvent, proc::BaselineInfo};

use super::partial_line::PartialLine;

pub struct EventList {
  state: ListState,
  events: Vec<Arc<TracerEvent>>,
  /// The string representation of the events, used for searching
  events_string: Vec<String>,
  /// Current window of the event list, [start, end)
  window: (usize, usize),
  /// Cache of the lines in the window
  lines_cache: VecDeque<Line<'static>>,
  should_refresh_lines_cache: bool,
  /// Cache of the list items in the view
  list_cache: List<'static>,
  should_refresh_list_cache: bool,
  /// How many items are there in the window
  nr_items_in_window: usize,
  horizontal_offset: usize,
  /// width that could be used for the list items(not including the selection indicator)
  inner_width: u16,
  /// max width of the lines in the current window
  max_width: usize,
  pub max_window_len: usize,
  pub baseline: Arc<BaselineInfo>,
  follow: bool,
  pub modifier_args: ModifierArgs,
  env_in_cmdline: bool,
}

impl EventList {
  pub fn new(baseline: BaselineInfo, follow: bool, modifier_args: ModifierArgs) -> Self {
    Self {
      state: ListState::default(),
      events: vec![],
      events_string: vec![],
      window: (0, 0),
      nr_items_in_window: 0,
      horizontal_offset: 0,
      inner_width: 0,
      max_width: 0,
      max_window_len: 0,
      baseline: Arc::new(baseline),
      follow,
      lines_cache: VecDeque::new(),
      should_refresh_lines_cache: true,
      should_refresh_list_cache: true,
      list_cache: List::default(),
      modifier_args,
      env_in_cmdline: true,
    }
  }

  pub fn is_env_in_cmdline(&self) -> bool {
    self.env_in_cmdline
  }

  pub fn is_following(&self) -> bool {
    self.follow
  }

  pub fn toggle_follow(&mut self) {
    self.follow = !self.follow;
  }

  pub fn stop_follow(&mut self) {
    self.follow = false;
  }

  pub fn toggle_env_display(&mut self) {
    self.env_in_cmdline = !self.env_in_cmdline;
    self.should_refresh_lines_cache = true;
  }

  /// returns the index of the selected item if there is any
  pub fn selection_index(&self) -> Option<usize> {
    self.state.selected().map(|i| self.window.0 + i)
  }

  /// returns the selected item if there is any
  pub fn selection(&self) -> Option<Arc<TracerEvent>> {
    self.selection_index().map(|i| self.events[i].clone())
  }

  /// Reset the window and force clear the line cache
  pub fn set_window(&mut self, window: (usize, usize)) {
    self.window = window;
    self.should_refresh_lines_cache = true;
  }

  pub fn get_window(&self) -> (usize, usize) {
    self.window
  }

  // TODO: this is ugly due to borrow checking.
  pub fn window(items: &[Arc<TracerEvent>], window: (usize, usize)) -> &[Arc<TracerEvent>] {
    &items[window.0..window.1.min(items.len())]
  }

  pub fn statistics(&self) -> Title {
    let id = self.selection_index().unwrap_or(0);
    Title::default()
      .content(format!(
        "{}/{}──",
        (id + 1).min(self.events.len()),
        self.events.len()
      ))
      .alignment(Right)
  }
}

impl Widget for &mut EventList {
  fn render(self, area: Rect, buf: &mut Buffer)
  where
    Self: Sized,
  {
    self.inner_width = area.width - 1; // 1 for the selection indicator
    let mut max_len = area.width as usize - 1;
    // Iterate through all elements in the `items` and stylize them.
    let events_in_window = EventList::window(&self.events, self.window);
    // tracing::debug!(
    //   "Should refresh line cache: {}",
    //   self.should_refresh_lines_cache
    // );
    if self.should_refresh_lines_cache {
      self.should_refresh_lines_cache = false;
      self.should_refresh_list_cache = true;
      // Initialize the line cache, which will be kept in sync by the navigation methods
      self.lines_cache = events_in_window
        .iter()
        .map(|evt| {
          evt.to_tui_line(
            &self.baseline,
            false,
            &self.modifier_args,
            self.env_in_cmdline,
          )
        })
        .collect();
    }
    self.nr_items_in_window = events_in_window.len();
    if self.nr_items_in_window > self.lines_cache.len() {
      // Push the new items to the cache
      self.should_refresh_list_cache = true;
      for evt in events_in_window.iter().skip(self.lines_cache.len()) {
        tracing::debug!("Pushing new item to line cache");
        self.lines_cache.push_back(evt.to_tui_line(
          &self.baseline,
          false,
          &self.modifier_args,
          self.env_in_cmdline,
        ));
      }
    }
    // tracing::debug!(
    //   "Should refresh list cache: {}",
    //   self.should_refresh_list_cache
    // );
    if self.should_refresh_list_cache {
      self.should_refresh_list_cache = false;
      let items = self.lines_cache.iter().map(|full_line| {
        max_len = max_len.max(full_line.width());
        ListItem::from(
          full_line
            .clone()
            .substring(self.horizontal_offset, area.width),
        )
      });
      // Create a List from all list items and highlight the currently selected one
      let list = List::new(items)
        .highlight_style(
          Style::default()
            .add_modifier(Modifier::BOLD)
            .bg(Color::DarkGray),
        )
        .highlight_symbol(">")
        .highlight_spacing(HighlightSpacing::Always);
      // FIXME: It's a little late to set the max width here. The max width is already used
      //        Though this should only affect the first render.
      self.max_width = max_len;
      self.list_cache = list;
    }

    // We can now render the item list
    // (look careful we are using StatefulWidget's render.)
    // ratatui::widgets::StatefulWidget::render as stateful_render
    StatefulWidgetRef::render_ref(&self.list_cache, area, buf, &mut self.state);

    // Render scrollbars
    if self.max_width + 1 > area.width as usize {
      // Render horizontal scrollbar, assuming there is a border we can overwrite
      let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom).thumb_symbol("■");
      let scrollbar_area = Rect {
        x: area.x,
        y: area.y + area.height,
        width: area.width,
        height: 1,
      };
      scrollbar.render(
        scrollbar_area,
        buf,
        &mut ScrollbarState::new(self.max_width + 1 - area.width as usize)
          .position(self.horizontal_offset),
      );
    }
    if self.events.len() > area.height as usize {
      // Render vertical scrollbar
      let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
      let scrollbar_area = Rect {
        x: area.x + area.width,
        y: area.y,
        width: 1,
        height: area.height,
      };
      scrollbar.render(
        scrollbar_area,
        buf,
        &mut ScrollbarState::new(self.events.len() - area.height as usize)
          .position(self.window.0 + self.state.selected().unwrap_or(0)),
      );
    }
  }
}

/// Event Management
impl EventList {
  pub fn push(&mut self, event: impl Into<Arc<TracerEvent>>) {
    let event = event.into();
    self.events_string.push(
      event
        .to_tui_line(
          &self.baseline,
          false,
          &self.modifier_args,
          self.env_in_cmdline,
        )
        .to_string(),
    );
    self.events.push(event.clone());
  }
}

/// Scrolling implementation for the EventList
impl EventList {
  /// Returns the index(absolute) of the last item in the window
  fn last_item_in_window_absolute(&self) -> Option<usize> {
    if self.events.is_empty() {
      return None;
    }
    Some(
      self
        .window
        .1
        .saturating_sub(1)
        .min(self.events.len().saturating_sub(1)),
    )
  }

  /// Returns the index(relative) of the last item in the window
  fn last_item_in_window_relative(&self) -> Option<usize> {
    if !self.events.is_empty() {
      Some(
        self
          .window
          .1
          .min(self.events.len())
          .saturating_sub(self.window.0)
          .saturating_sub(1),
      )
    } else {
      None
    }
  }

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

  /// Try to slide down the window by one item
  /// Returns true if the window was slid down, false otherwise
  pub fn next_window(&mut self) -> bool {
    if self.events.is_empty() {
      return false;
    }
    if self.window.1 < self.events.len() {
      self.window.0 += 1;
      self.window.1 += 1;
      self.lines_cache.pop_front();
      self.lines_cache.push_back(
        self.events[self.last_item_in_window_absolute().unwrap()].to_tui_line(
          &self.baseline,
          false,
          &self.modifier_args,
          self.env_in_cmdline,
        ),
      );
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
      self.lines_cache.pop_back();
      self
        .lines_cache
        .push_front(self.events[self.window.0].to_tui_line(
          &self.baseline,
          false,
          &self.modifier_args,
          self.env_in_cmdline,
        ));
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

  pub fn page_down(&mut self) {
    if self.window.1 + self.max_window_len <= self.events.len() {
      self.window.0 += self.max_window_len;
      self.window.1 += self.max_window_len;
      self.should_refresh_lines_cache = true;
    } else {
      // If we can't slide down the window by the number of items in the window
      // just set the window to the last items
      let old_window = self.window;
      self.window.0 = self.events.len().saturating_sub(self.max_window_len);
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_lines_cache = old_window != self.window;
    }
    self.state.select(self.last_item_in_window_relative());
    tracing::trace!(
      "pgdn: should_refresh_lines_cache = {}",
      self.should_refresh_lines_cache
    );
  }

  pub fn page_up(&mut self) {
    // Try to slide up the window by the number of items in the window
    if self.window.0 >= self.max_window_len {
      self.window.0 -= self.max_window_len;
      self.window.1 -= self.max_window_len;
      self.should_refresh_lines_cache = true;
    } else {
      // If we can't slide up the window by the number of items in the window
      // just set the window to the first items
      let old_window = self.window;
      self.window.0 = 0;
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_lines_cache = old_window != self.window;
    }
    self.select_first();
    tracing::trace!(
      "pgup: should_refresh_lines_cache = {}",
      self.should_refresh_lines_cache
    );
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

  pub fn scroll_to_top(&mut self) {
    let old_window = self.window;
    self.window.0 = 0;
    self.window.1 = self.max_window_len;
    self.should_refresh_lines_cache = old_window != self.window;
    self.select_first();
    tracing::trace!(
      "top: should_refresh_lines_cache = {}",
      self.should_refresh_lines_cache
    );
  }

  pub fn scroll_to_bottom(&mut self) {
    if self.events.is_empty() {
      return;
    }
    let old_window = self.window;
    self.window.0 = self.events.len().saturating_sub(self.max_window_len);
    self.window.1 = self.window.0 + self.max_window_len;
    self.select_last();
    if self.window.0.saturating_sub(old_window.0) == 1
      && self.window.1.saturating_sub(old_window.1) == 1
    {
      // Special optimization for follow mode where scroll to bottom is called continuously
      self.lines_cache.pop_front();
      self.lines_cache.push_back(
        self.events[self.last_item_in_window_absolute().unwrap()].to_tui_line(
          &self.baseline,
          false,
          &self.modifier_args,
          self.env_in_cmdline,
        ),
      );
      self.should_refresh_list_cache = true;
    } else {
      self.should_refresh_lines_cache = old_window != self.window;
      tracing::trace!(
        "bottom: should_refresh_lines_cache = {}",
        self.should_refresh_lines_cache
      );
    }
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
