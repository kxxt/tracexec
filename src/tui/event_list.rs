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
use nix::sys::signal::Signal;
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
use tracing::trace;

use crate::{
  cli::args::ModifierArgs,
  event::{EventStatus, ProcessStateUpdate, ProcessStateUpdateEvent, RuntimeModifier, TracerEventDetails},
  proc::BaselineInfo,
  tracer::state::ProcessExit,
};

use super::{
  event_line::EventLine, partial_line::PartialLine, query::{Query, QueryResult}, theme::THEME
};

pub struct Event {
  pub details: Arc<TracerEventDetails>,
  pub status: Option<EventStatus>,
}

impl Event {
  fn to_tui_line(&self, list: &EventList) -> Line<'static> {
    self.details.to_tui_line(
      &list.baseline,
      false,
      &list.modifier_args,
      list.runtime_modifier(),
      self.status,
    )
  }
}

pub struct EventList {
  state: ListState,
  events: Vec<Event>,
  /// The string representation of the events, used for searching
  event_lines: Vec<EventLine>,
  /// Current window of the event list, [start, end)
  window: (usize, usize),
  /// Cache of the (index, line)s in the window
  lines_cache: VecDeque<(usize, Line<'static>)>,
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
  rt_modifier: RuntimeModifier,
  query: Option<Query>,
  query_result: Option<QueryResult>,
}

impl EventList {
  pub fn new(baseline: BaselineInfo, follow: bool, modifier_args: ModifierArgs) -> Self {
    Self {
      state: ListState::default(),
      events: vec![],
      event_lines: vec![],
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
    self.should_refresh_lines_cache = true;
    self.rebuild_event_strings();
  }

  pub fn toggle_cwd_display(&mut self) {
    self.rt_modifier.show_cwd = !self.rt_modifier.show_cwd;
    self.should_refresh_lines_cache = true;
    self.rebuild_event_strings();
  }

  /// returns the index of the selected item if there is any
  pub fn selection_index(&self) -> Option<usize> {
    self.state.selected().map(|i| self.window.0 + i)
  }

  /// returns the selected item if there is any
  pub fn selection(&self) -> Option<&Event> {
    self.selection_index().map(|i| &self.events[i])
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
  pub fn window(items: &[Event], window: (usize, usize)) -> &[Event] {
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
        .enumerate()
        .map(|(i, evt)| (i + self.window.0, evt.to_tui_line(self)))
        .collect();
    }
    self.nr_items_in_window = events_in_window.len();
    if self.nr_items_in_window > self.lines_cache.len() {
      // Push the new items to the cache
      self.should_refresh_list_cache = true;
      for (i, evt) in events_in_window
        .iter()
        .enumerate()
        .skip(self.lines_cache.len())
      {
        tracing::debug!("Pushing new item to line cache");
        self.lines_cache.push_back((i, evt.to_tui_line(self)));
      }
    }
    // tracing::debug!(
    //   "Should refresh list cache: {}",
    //   self.should_refresh_list_cache
    // );
    if self.should_refresh_list_cache {
      self.should_refresh_list_cache = false;
      tracing::debug!("Refreshing list cache");
      let items = self.lines_cache.iter().map(|(i, full_line)| {
        max_len = max_len.max(full_line.width());
        let highlighted = self
          .query_result
          .as_ref()
          .map_or(false, |query_result| query_result.indices.contains_key(i));
        let mut base = full_line
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
    for (i, evt) in self.event_lines.iter().enumerate() {
      if query.matches(evt) {
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
      .event_lines
      .iter()
      .enumerate()
      .skip(existing_result.searched_len)
    {
      if query.matches(evt) {
        existing_result.indices.insert(i, 0);
        modified = true;
      }
    }
    existing_result.searched_len = self.event_lines.len();
    if modified {
      self.should_refresh_list_cache = true;
    }
  }

  pub fn next_match(&mut self) {
    if let Some(query_result) = self.query_result.as_mut() {
      query_result.next_result();
      let selection = query_result.selection();
      self.scroll_to(selection);
    }
  }

  pub fn prev_match(&mut self) {
    if let Some(query_result) = self.query_result.as_mut() {
      query_result.prev_result();
      let selection = query_result.selection();
      self.scroll_to(selection);
    }
  }
}

/// Event Management
impl EventList {
  pub fn push(&mut self, event: impl Into<Arc<TracerEventDetails>>) {
    let event = event.into();
    let event = Event {
      status: match event.as_ref() {
        TracerEventDetails::NewChild { .. } => Some(EventStatus::ProcessRunning),
        TracerEventDetails::Exec(exec) => {
          match exec.result {
            0 => Some(EventStatus::ProcessRunning),
            -2 => Some(EventStatus::ExecENOENT), // ENOENT
            _ => Some(EventStatus::ExecFailure),
          }
        }
        _ => None,
      },
      details: event,
    };
    self.event_lines.push(event.to_tui_line(self).into());
    self.events.push(event);
    self.incremental_search();
  }

  pub fn update(&mut self, update: ProcessStateUpdateEvent) {
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
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::SIGTERM)) => {
          Some(EventStatus::ProcessTerminated)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::SIGKILL)) => {
          Some(EventStatus::ProcessKilled)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::SIGINT)) => {
          Some(EventStatus::ProcessInterrupted)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::SIGSEGV)) => {
          Some(EventStatus::ProcessSegfault)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::SIGABRT)) => {
          Some(EventStatus::ProcessAborted)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(Signal::SIGILL)) => {
          Some(EventStatus::ProcessIllegalInstruction)
        }
        ProcessStateUpdate::Exit(ProcessExit::Signal(s)) => Some(EventStatus::ProcessSignaled(s)),
      };
      self.event_lines[i] = self.events[i].to_tui_line(self).into();
      trace!(
        "window: {:?}, i: {}, cache: {}",
        self.window,
        i,
        self.lines_cache.len()
      );
      if self.window.0 <= i && i < self.window.1 {
        let j = i - self.window.0;
        if j < self.lines_cache.len() {
          self.lines_cache[j] = (i, self.events[i].to_tui_line(self));
        } else {
          // The line might not be in cache if the current window is not full and this event is a
          // new one, which will be added to the cache later.
        }
        self.should_refresh_list_cache = true;
      }
    }
  }

  pub fn rebuild_event_strings(&mut self) {
    self.event_lines = self
      .events
      .iter()
      .map(|evt| evt.to_tui_line(self).into())
      .collect();
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
      self.should_refresh_lines_cache = true;
      self.state.select(Some(0));
    } else if index >= self.window.1 {
      // Scroll down
      self.window.0 = index.min(self.events.len().saturating_sub(self.max_window_len));
      self.window.1 = self.window.0 + self.max_window_len;
      self.should_refresh_lines_cache = true;
      self.state.select(Some(index - self.window.0));
    } else {
      self.state.select(Some(index - self.window.0));
    }
  }

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
      let last_index = self.last_item_in_window_absolute().unwrap();
      self
        .lines_cache
        .push_back((last_index, self.events[last_index].to_tui_line(self)));
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
      let front_index = self.window.0;
      self
        .lines_cache
        .push_front((front_index, self.events[front_index].to_tui_line(self)));
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
      let last_index = self.last_item_in_window_absolute().unwrap();
      self
        .lines_cache
        .push_back((last_index, self.events[last_index].to_tui_line(self)));
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
