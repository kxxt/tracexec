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

use std::{
  cell::RefCell,
  collections::VecDeque,
  rc::Rc,
  sync::Arc,
};

use chrono::TimeDelta;
use hashbrown::HashMap;
use ratatui::{
  text::Span,
  widgets::{
    List,
    ListState,
  },
};
use tracexec_core::{
  cli::args::ModifierArgs,
  event::{
    EventId,
    EventStatus,
    ParentEvent,
    ProcessStateUpdateEvent,
    RuntimeModifier,
    TracerEventDetails,
  },
  proc::BaselineInfo,
};
use ui::pstate_update_to_status;

use super::{
  event_line::EventLine,
  query::{
    Query,
    QueryResult,
  },
};

mod react;
mod scroll;
mod ui;

#[derive(Debug, Clone)]
pub struct Event {
  pub details: Arc<TracerEventDetails>,
  pub status: Option<EventStatus>,
  /// The elapsed time between event start and process exit/detach.
  pub elapsed: Option<TimeDelta>,
  pub id: EventId,
}

#[derive(Debug)]
struct EventLocalStorage {
  pub extra_prefix: Option<Span<'static>>,
  pub line: EventLine,
}

pub struct EventModifier {
  modifier_args: ModifierArgs,
  rt_modifier: RuntimeModifier,
}

#[derive(Debug)]
pub struct EventList {
  state: ListState,
  // TODO: move the event id out of RwLock
  events: VecDeque<Rc<RefCell<Event>>>,
  /// 0. The string representation of the events, used for searching
  event_map: HashMap<EventId, (EventLocalStorage, Rc<RefCell<Event>>)>,
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
  pub(super) rt_modifier: RuntimeModifier,
  query: Option<Query>,
  query_result: Option<QueryResult>,
  is_ptrace: bool,
  pub(super) has_clipboard: bool,
  /// Whether this is the primary event list(a.k.a the one shown at start up),
  /// or a secondary one(e.g. a subset event list embedded in a popup)
  is_primary: bool,
}

impl EventList {
  pub fn new(
    baseline: Arc<BaselineInfo>,
    follow: bool,
    modifier_args: ModifierArgs,
    max_events: u64,
    is_ptrace: bool,
    has_clipboard: bool,
    is_primary: bool,
  ) -> Self {
    Self {
      state: ListState::default(),
      events: VecDeque::new(),
      event_map: HashMap::new(),
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
      is_ptrace,
      has_clipboard,
      is_primary,
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
    for event in &self.events {
      self
        .event_map
        .get_mut(&event.borrow().id)
        .unwrap()
        .0
        .line
        .toggle_env_mask();
    }
    self.should_refresh_list_cache = true;
    self.search();
  }

  pub fn toggle_cwd_display(&mut self) {
    self.rt_modifier.show_cwd = !self.rt_modifier.show_cwd;
    for event in &mut self.events {
      self
        .event_map
        .get_mut(&event.borrow().id)
        .unwrap()
        .0
        .line
        .toggle_cwd_mask();
    }
    self.should_refresh_list_cache = true;
    self.search();
  }

  /// returns the index of the selected item if there is any
  pub fn selection_index(&self) -> Option<usize> {
    self.state.selected().map(|i| self.window.0 + i)
  }

  pub(super) fn selection(&self) -> Option<Rc<RefCell<Event>>> {
    self.selection_index().map(|i| self.events[i].clone())
  }

  /// returns the selected item if there is any
  pub fn selection_map<T>(&self, f: impl FnOnce(&Event) -> T) -> Option<T> {
    self.selection_index().map(|i| {
      let e = self.events[i].borrow();
      f(&e)
    })
  }

  /// Reset the window and force clear the list cache
  pub fn set_window(&mut self, window: (usize, usize)) {
    self.window = window;
    self.should_refresh_list_cache = true;
  }

  pub fn get_window(&self) -> (usize, usize) {
    self.window
  }

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

  pub fn len(&self) -> usize {
    self.events.len()
  }

  pub fn contains(&self, id: EventId) -> bool {
    self.event_map.contains_key(&id)
  }

  pub(super) fn get(&self, id: EventId) -> Option<Rc<RefCell<Event>>> {
    self.event_map.get(&id).map(|(_, x)| Rc::clone(x))
  }

  pub fn get_map<T>(&self, id: EventId, f: impl FnOnce(&Event) -> T) -> Option<T> {
    self.event_map.get(&id).map(|e| {
      let e = e.1.borrow();
      f(&e)
    })
  }

  pub fn get_parent(&self, id: EventId) -> Option<ParentEvent<Rc<RefCell<Event>>>> {
    let parent_id = self.event_map.get(&id).and_then(|(_, e)| {
      if let TracerEventDetails::Exec(exec) = e.borrow().details.as_ref() {
        exec.parent
      } else {
        None
      }
    });
    if let Some(p) = parent_id {
      p.map(|id| self.event_map.get(&id).map(|(_, e)| e.clone()))
        .transpose()
    } else {
      None
    }
  }
}

/// Query Management
impl EventList {
  pub async fn set_query(&mut self, query: Option<Query>) {
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
    let mut indices = indexset::BTreeSet::new();
    // Events won't change during the search because this is Rust and we already have a reference to it.
    // Rust really makes the code more easier to reason about.
    for evt in self.events.iter() {
      let id = evt.borrow().id;
      if query.matches(&self.event_map[&id].0.line) {
        indices.insert(id);
      }
    }
    let mut result = QueryResult {
      indices,
      searched_id: self
        .events
        .iter()
        .last()
        .map(|r| r.borrow().id)
        .unwrap_or_else(EventId::zero),
      selection: None,
    };
    result.next_result();
    let selection = result.selection();
    self.query_result = Some(result);
    self.should_refresh_list_cache = true;
    self.scroll_to_id(selection);
  }

  /// Incremental search for newly added events
  pub fn incremental_search(&mut self) {
    let Some(query) = self.query.as_ref() else {
      return;
    };
    let offset = self.id_index_offset();
    let Some(existing_result) = self.query_result.as_mut() else {
      self.search();
      return;
    };
    let mut modified = false;
    let start_search_index = existing_result
      .searched_id
      .into_inner()
      .saturating_sub(offset) as usize;
    for evt in self.events.iter().skip(start_search_index) {
      let id = evt.borrow().id;
      if query.matches(&self.event_map[&id].0.line) {
        existing_result.indices.insert(id);
        modified = true;
      }
    }
    existing_result.searched_id = self
      .events
      .iter()
      .last()
      .map(|r| r.borrow().id)
      .unwrap_or_else(EventId::zero);
    if modified {
      self.should_refresh_list_cache = true;
    }
  }

  pub async fn next_match(&mut self) {
    if let Some(query_result) = self.query_result.as_mut() {
      query_result.next_result();
      let selection = query_result.selection();
      self.stop_follow();
      self.scroll_to_id(selection);
    }
  }

  pub async fn prev_match(&mut self) {
    if let Some(query_result) = self.query_result.as_mut() {
      query_result.prev_result();
      let selection = query_result.selection();
      self.stop_follow();
      self.scroll_to_id(selection);
    }
  }
}

/// Event Management
impl EventList {
  /// Push a new event into event list.
  ///
  /// Caller must guarantee that the id is strict monotonically increasing.
  pub fn push(&mut self, id: EventId, event: impl Into<Arc<TracerEventDetails>>) {
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
      elapsed: None,
      details,
      status,
      id,
    };
    let line = event.to_event_line(&self.baseline, &self.event_modifier(), None);
    if self.events.len() >= self.max_events as usize {
      if let Some(e) = self.events.pop_front() {
        let id = e.borrow().id;
        self.event_map.remove(&id);
        if let Some(q) = &mut self.query_result {
          q.indices.remove(&id);
        }
      }
      self.should_refresh_list_cache = true;
    }
    let event = Rc::new(RefCell::new(event));
    self.events.push_back(event.clone());
    // # SAFETY
    //
    // The event ids are guaranteed to be unique
    unsafe {
      self.event_map.insert_unique_unchecked(
        id,
        (
          EventLocalStorage {
            line,
            extra_prefix: None,
          },
          event,
        ),
      )
    };
    self.incremental_search();
    if (self.window.0..self.window.1).contains(&(self.events.len() - 1)) {
      self.should_refresh_list_cache = true;
    }
  }

  pub fn set_status(&mut self, id: EventId, status: Option<EventStatus>) -> Option<()> {
    self
      .event_map
      .get_mut(&id)
      .map(|v| v.1.borrow_mut())?
      .status = status;
    Some(())
  }

  /// Directly push [`Event`] into the list without
  /// - Checking `max_events` constraint
  /// - Maintaining query result
  pub(super) fn dumb_push(
    &mut self,
    event: Rc<RefCell<Event>>,
    extra_prefix: Option<Span<'static>>,
  ) {
    let id = event.borrow().id;
    self.events.push_back(event.clone());
    let evt = event.borrow();
    let line = evt.to_event_line(&self.baseline, &self.event_modifier(), extra_prefix.clone());
    drop(evt);
    // # SAFETY
    //
    // The event ids are guaranteed to be unique
    unsafe {
      self
        .event_map
        .insert_unique_unchecked(id, (EventLocalStorage { line, extra_prefix }, event))
    };
  }

  pub fn update(&mut self, update: ProcessStateUpdateEvent) {
    let modifier = self.event_modifier();
    for i in update.ids {
      if let Some((storage, e)) = self.event_map.get_mut(&i) {
        let mut e = e.borrow_mut();
        if let TracerEventDetails::Exec(exec) = e.details.as_ref()
          && exec.result != 0
        {
          // Don't update the status for failed exec events
          continue;
        }
        e.status = pstate_update_to_status(&update.update);
        if let Some(ts) = update.update.termination_timestamp() {
          e.elapsed = Some(ts - e.details.timestamp().unwrap())
        }
        // FIXME: currently we do not handle event status updates in secondary event lists.
        storage.line = e.to_event_line(&self.baseline, &modifier, storage.extra_prefix.clone());
      }
      let i = i.into_inner() as usize;
      if self.window.0 <= i && i < self.window.1 {
        self.should_refresh_list_cache = true;
      }
    }
  }

  pub fn rebuild_lines(&mut self) {
    // TODO: only update spans that are affected by the change
    let modifier = self.event_modifier();
    for (_, (storage, e)) in self.event_map.iter_mut() {
      let e = e.borrow();
      storage.line = e.to_event_line(&self.baseline, &modifier, storage.extra_prefix.clone());
    }
    self.should_refresh_list_cache = true;
  }

  fn event_modifier(&self) -> EventModifier {
    EventModifier {
      modifier_args: self.modifier_args.clone(),
      rt_modifier: self.rt_modifier,
    }
  }
}

/// Scrolling implementation for the EventList
impl EventList {
  fn id_index_offset(&self) -> u64 {
    self
      .events
      .get(self.window.0)
      .map(|e| e.borrow().id)
      .unwrap_or_else(EventId::zero)
      .into_inner()
      .saturating_sub(self.window.0 as u64)
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
