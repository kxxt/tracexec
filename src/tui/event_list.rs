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

use indexmap::IndexMap;
use nix::sys::signal;
use ratatui::{
  layout::Alignment::Right,
  prelude::{Buffer, Rect},
  style::{Color, Modifier, Style},
  text::Line,
  widgets::{
    HighlightSpacing, List, ListItem, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState,
    StatefulWidget, StatefulWidgetRef, Widget,
  },
};

use crate::{
  cli::args::ModifierArgs,
  event::{
    EventStatus, ProcessStateUpdate, ProcessStateUpdateEvent, RuntimeModifier, TracerEventDetails,
  },
  proc::BaselineInfo,
  ptrace::Signal,
  tracer::state::ProcessExit,
};

use super::{
  event_line::EventLine,
  partial_line::PartialLine,
  query::{Query, QueryResult},
  theme::THEME,
};

pub struct Event {
  pub details: Arc<TracerEventDetails>,
  pub status: Option<EventStatus>,
  /// The string representation of the events, used for searching
  pub event_line: EventLine,
}

pub struct EventModifier {
  modifier_args: ModifierArgs,
  rt_modifier: RuntimeModifier,
}

impl Event {
  fn to_event_line(
    details: &TracerEventDetails,
    status: Option<EventStatus>,
    baseline: &BaselineInfo,
    modifier: &EventModifier,
  ) -> EventLine {
    details.to_event_line(
      &baseline,
      false,
      &modifier.modifier_args,
      modifier.rt_modifier,
      status,
      true,
    )
  }
}

pub struct EventList {
  state: ListState,
  events: VecDeque<Event>,
  /// Current window of the event list, [start, end)
  window: (usize, usize),
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
  max_events: u64,
  pub max_window_len: usize,
  pub baseline: Arc<BaselineInfo>,
  follow: bool,
  pub modifier_args: ModifierArgs,
  rt_modifier: RuntimeModifier,
  query: Option<Query>,
  query_result: Option<QueryResult>,
}

impl EventList {
  pub fn new(
    baseline: Arc<BaselineInfo>,
    follow: bool,
    modifier_args: ModifierArgs,
    max_events: u64,
  ) -> Self {
    Self {
      state: ListState::default(),
      events: VecDeque::new(),
      window: (0, 0),
      nr_items_in_window: 0,
      horizontal_offset: 0,
      inner_width: 0,
      max_width: 0,
      max_window_len: 0,
      max_events,
      baseline,
      follow,
      should_refresh_list_cache: true,
      list_cache: List::default(),
      modifier_args,
      rt_modifier: Default::default(),
      query: None,
      query_result: None,
    }
  }

  pub fn runtime_modifier(&self) -> RuntimeModifier {
    self.rt_modifier
  }

  pub fn is_env_in_cmdline(&self) -> bool {
    self.rt_modifier.show_env
  }

  pub fn is_cwd_in_cmdline(&self) -> bool {
    self.rt_modifier.show_cwd
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
    self.rt_modifier.show_env = !self.rt_modifier.show_env;
    for event in &mut self.events {
      if let Some(mask) = &mut event.event_line.env_mask {
        mask.toggle(&mut event.event_line.line);
      }
    }
    self.should_refresh_list_cache = true;
    self.search();
  }

  pub fn toggle_cwd_display(&mut self) {
    self.rt_modifier.show_cwd = !self.rt_modifier.show_cwd;
    for event in &mut self.events {
      if let Some(mask) = &mut event.event_line.cwd_mask {
        mask.toggle(&mut event.event_line.line);
      }
    }
    self.should_refresh_list_cache = true;
    self.search();
  }

  /// returns the index of the selected item if there is any
  pub fn selection_index(&self) -> Option<usize> {
    self.state.selected().map(|i| self.window.0 + i)
  }

  /// returns the selected item if there is any
  pub fn selection(&self) -> Option<&Event> {
    self.selection_index().map(|i| &self.events[i])
  }

  /// Reset the window and force clear the list cache
  pub fn set_window(&mut self, window: (usize, usize)) {
    self.window = window;
    self.should_refresh_list_cache = true;
  }

  pub fn get_window(&self) -> (usize, usize) {
    self.window
  }

  // TODO: this is ugly due to borrow checking.
  pub fn window<'a, T>(items: (&'a [T], &'a [T]), window: (usize, usize)) -> (&'a [T], &'a [T]) {
    let end = window.1.min(items.0.len() + items.1.len());
    let separation = items.0.len();
    if window.0 >= separation {
      (&[], &items.1[(window.0 - separation)..(end - separation)])
    } else if end > separation {
      (
        &items.0[window.0..separation],
        &items.1[..(end - separation)],
      )
    } else {
      (&items.0[window.0..end], [].as_slice())
    }
  }

  pub fn statistics(&self) -> Line {
    let id = self.selection_index().unwrap_or(0);
    Line::raw(format!(
      "{}/{}──",
      (id + 1).min(self.events.len()),
      self.events.len()
    ))
    .alignment(Right)
  }

  pub fn len(&self) -> usize {
    self.events.len()
  }
}

impl Widget for &mut EventList {
  fn render(self, area: Rect, buf: &mut Buffer)
  where
    Self: Sized,
  {
    self.inner_width = area.width - 2; // for the selection indicator
    let mut max_len = area.width as usize - 1;
    // Iterate through all elements in the `items` and stylize them.
    let events_in_window = EventList::window(self.events.as_slices(), self.window);
    self.nr_items_in_window = events_in_window.0.len() + events_in_window.1.len();
    // tracing::debug!(
    //   "Should refresh list cache: {}",
    //   self.should_refresh_list_cache
    // );
    if self.should_refresh_list_cache {
      self.should_refresh_list_cache = false;
      tracing::debug!("Refreshing list cache");
      let items = self
        .events
        .iter()
        .enumerate()
        .skip(self.window.0)
        .take(self.window.1 - self.window.0)
        .map(|(i, event)| {
          max_len = max_len.max(event.event_line.line.width());
          let highlighted = self
            .query_result
            .as_ref()
            .is_some_and(|query_result| query_result.indices.contains_key(&i));
          let mut base = event
            .event_line
            .line
            .clone()
            .substring(self.horizontal_offset, area.width);
          if highlighted {
            base = base.style(THEME.search_match);
          }
          ListItem::from(base)
        });
      // Create a List from all list items and highlight the currently selected one
      let list = List::new(items)
        .highlight_style(
          Style::default()
            .add_modifier(Modifier::BOLD)
            .bg(Color::DarkGray),
        )
        .highlight_symbol("➡️")
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

    if let Some(query_result) = self.query_result.as_ref() {
      let statistics = query_result.statistics();
      let statistics_len = statistics.width();
      if statistics_len > buf.area().width as usize {
        return;
      }
      let statistics_area = Rect {
        x: buf.area().right().saturating_sub(statistics_len as u16),
        y: 1,
        width: statistics_len as u16,
        height: 1,
      };
      statistics.render(statistics_area, buf);
    }
  }
}

/// Query Management
impl EventList {
  pub fn set_query(&mut self, query: Option<Query>) {
    if query.is_some() {
      self.query = query;
      self.search();
    } else {
      self.query = None;
      self.query_result = None;
      self.should_refresh_list_cache = true;
    }
  }

  /// Search for the query in the event list
  /// And update query result,
  /// Then set the selection to the first result(if any) and scroll to it
  pub fn search(&mut self) {
    let Some(query) = self.query.as_ref() else {
      return;
    };
    let mut indices = IndexMap::new();
    // Events won't change during the search because this is Rust and we already have a reference to it.
    // Rust really makes the code more easier to reason about.
    let searched_len = self.events.len();
    for (i, evt) in self.events.iter().enumerate() {
      if query.matches(&evt.event_line) {
        indices.insert(i, 0);
      }
    }
    let mut result = QueryResult {
      indices,
      searched_len,
      selection: None,
    };
    result.next_result();
    let selection = result.selection();
    self.query_result = Some(result);
    self.should_refresh_list_cache = true;
    self.scroll_to(selection);
  }

  /// Incremental search for newly added events
  pub fn incremental_search(&mut self) {
    let Some(query) = self.query.as_ref() else {
      return;
    };
    let Some(existing_result) = self.query_result.as_mut() else {
      self.search();
      return;
    };
    let mut modified = false;
    for (i, evt) in self
      .events
      .iter()
      .enumerate()
      .skip(existing_result.searched_len)
    {
      if query.matches(&evt.event_line) {
        existing_result.indices.insert(i, 0);
        modified = true;
      }
    }
    existing_result.searched_len = self.events.len();
    if modified {
      self.should_refresh_list_cache = true;
    }
  }

  pub fn next_match(&mut self) {
    if let Some(query_result) = self.query_result.as_mut() {
      query_result.next_result();
      let selection = query_result.selection();
      self.scroll_to(selection);
      self.stop_follow();
    }
  }

  pub fn prev_match(&mut self) {
    if let Some(query_result) = self.query_result.as_mut() {
      query_result.prev_result();
      let selection = query_result.selection();
      self.scroll_to(selection);
      self.stop_follow();
    }
  }
}

/// Event Management
impl EventList {
  pub fn push(&mut self, event: impl Into<Arc<TracerEventDetails>>) {
    let details = event.into();
    let status = match details.as_ref() {
      TracerEventDetails::NewChild { .. } => Some(EventStatus::ProcessRunning),
      TracerEventDetails::Exec(exec) => {
        match exec.result {
          0 => Some(EventStatus::ProcessRunning),
          -2 => Some(EventStatus::ExecENOENT), // ENOENT
          _ => Some(EventStatus::ExecFailure),
        }
      }
      _ => None,
    };
    let event = Event {
      event_line: Event::to_event_line(&details, status, &self.baseline, &self.event_modifier()),
      details,
      status,
    };
    if self.events.len() >= self.max_events as usize {
      self.events.pop_front();
      self.should_refresh_list_cache = true;
    }
    self.events.push_back(event);
    self.incremental_search();
    if (self.window.0..self.window.1).contains(&(self.events.len() - 1)) {
      self.should_refresh_list_cache = true;
    }
  }

  pub fn update(&mut self, update: ProcessStateUpdateEvent) {
    let modifier = self.event_modifier();
    for i in update.ids {
      let i = i as usize;
      if let TracerEventDetails::Exec(exec) = self.events[i].details.as_ref() {
        if exec.result != 0 {
          // Don't update the status for failed exec events
          continue;
        }
      }
      self.events[i].status = match update.update {
        ProcessStateUpdate::Exit(ProcessExit::Code(0)) => Some(EventStatus::ProcessExitedNormally),
        ProcessStateUpdate::Exit(ProcessExit::Code(c)) => {
          Some(EventStatus::ProcessExitedAbnormally(c))
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::Standard(signal::SIGTERM))) => {
          Some(EventStatus::ProcessTerminated)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::Standard(signal::SIGKILL))) => {
          Some(EventStatus::ProcessKilled)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::Standard(signal::SIGINT))) => {
          Some(EventStatus::ProcessInterrupted)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::Standard(signal::SIGSEGV))) => {
          Some(EventStatus::ProcessSegfault)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::Standard(signal::SIGABRT))) => {
          Some(EventStatus::ProcessAborted)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::Standard(signal::SIGILL))) => {
          Some(EventStatus::ProcessIllegalInstruction)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(s)) => Some(EventStatus::ProcessSignaled(s)),
        ProcessStateUpdate::BreakPointHit { .. } => Some(EventStatus::ProcessPaused),
        ProcessStateUpdate::Resumed => Some(EventStatus::ProcessRunning),
        ProcessStateUpdate::Detached { .. } => Some(EventStatus::ProcessDetached),
        _ => unimplemented!(),
      };
      self.events[i].event_line = Event::to_event_line(
        &self.events[i].details,
        self.events[i].status,
        &self.baseline,
        &modifier,
      );
      if self.window.0 <= i && i < self.window.1 {
        self.should_refresh_list_cache = true;
      }
    }
  }

  pub fn rebuild_lines(&mut self) {
    // TODO: only update spans that are affected by the change
    let modifier = self.event_modifier();
    for e in self.events.iter_mut() {
      e.event_line = Event::to_event_line(&e.details, e.status, &self.baseline, &modifier);
    }
    self.should_refresh_list_cache = true;
  }

  fn event_modifier(&self) -> EventModifier {
    EventModifier {
      modifier_args: self.modifier_args,
      rt_modifier: self.rt_modifier,
    }
  }
}

/// Scrolling implementation for the EventList
impl EventList {
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

  /// Returns the index(absolute) of the last item in the window
  #[allow(dead_code)]
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

#[cfg(test)]

mod test {
  use super::EventList;

  #[test]
  fn test_window_with_valid_input() {
    let items = (&[1, 2, 3] as &[i32], &[4, 5, 6] as &[i32]);
    let window = (1, 4);

    let result = EventList::window(items, window);

    assert_eq!(result.0, &[2, 3]);
    assert_eq!(result.1, &[4]);

    let result = EventList::window(items, (3, 5));

    assert_eq!(result.0, &[] as &[i32]);
    assert_eq!(result.1, &[4, 5] as &[i32]);

    let result = EventList::window(items, (0, 2));

    assert_eq!(result.0, &[1, 2] as &[i32]);
    assert_eq!(result.1, &[] as &[i32]);
  }

  #[test]
  fn test_window_with_empty_slices() {
    let items = (&[] as &[i32], &[] as &[i32]);
    let window = (0, 2);

    let result = EventList::window(items, window);

    assert_eq!(result.0, &[] as &[i32]);
    assert_eq!(result.1, &[] as &[i32]);
  }

  #[test]
  fn test_window_with_out_of_bounds_window() {
    let items = (&[1, 2] as &[i32], &[3, 4, 5] as &[i32]);
    let window = (3, 7);

    let result = EventList::window(items, window);

    assert_eq!(result.0, &[] as &[i32]);
    assert_eq!(result.1, &[4, 5]);
  }

  #[test]
  fn test_window_with_zero_length_window() {
    let items = (&[1, 2, 3] as &[i32], &[4, 5, 6] as &[i32]);
    let window = (2, 2);

    let result = EventList::window(items, window);

    assert_eq!(result.0, &[] as &[i32]);
    assert_eq!(result.1, &[] as &[i32]);
  }
}
