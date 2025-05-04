use nix::sys::signal;
use ratatui::{
  buffer::Buffer,
  layout::{Alignment, Rect},
  style::{Color, Modifier, Style},
  text::Line,
  widgets::{
    HighlightSpacing, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState,
    StatefulWidget, StatefulWidgetRef, Widget,
  },
};

use crate::{
  event::{EventStatus, ProcessStateUpdate, TracerEventDetails},
  proc::BaselineInfo,
  ptrace::Signal,
  tracer::ProcessExit,
  tui::{event_line::EventLine, partial_line::PartialLine, theme::THEME},
};

use super::{Event, EventList, EventModifier};

impl Event {
  pub(super) fn to_event_line(
    details: &TracerEventDetails,
    status: Option<EventStatus>,
    baseline: &BaselineInfo,
    modifier: &EventModifier,
  ) -> EventLine {
    details.to_event_line(
      baseline,
      false,
      &modifier.modifier_args,
      modifier.rt_modifier,
      status,
      true,
    )
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
        .skip(self.window.0)
        .take(self.window.1 - self.window.0)
        .map(|event| {
          // FIXME
          let event = futures::executor::block_on(event.read());
          max_len = max_len.max(event.event_line.line.width());
          let highlighted = self
            .query_result
            .as_ref()
            .is_some_and(|query_result| query_result.indices.contains(&event.id));
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

impl EventList {
  pub fn statistics(&self) -> Line {
    let id = self.selection_index().unwrap_or(0);
    Line::raw(format!(
      "{}/{}──",
      (id + 1).min(self.events.len()),
      self.events.len()
    ))
    .alignment(Alignment::Right)
  }
}

pub(super) fn pstate_update_to_status(update: &ProcessStateUpdate) -> Option<EventStatus> {
  match update {
    ProcessStateUpdate::Exit {
      status: ProcessExit::Code(0),
      ..
    } => Some(EventStatus::ProcessExitedNormally),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Code(c),
      ..
    } => Some(EventStatus::ProcessExitedAbnormally(*c)),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(Signal::Standard(signal::SIGTERM)),
      ..
    } => Some(EventStatus::ProcessTerminated),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(Signal::Standard(signal::SIGKILL)),
      ..
    } => Some(EventStatus::ProcessKilled),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(Signal::Standard(signal::SIGINT)),
      ..
    } => Some(EventStatus::ProcessInterrupted),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(Signal::Standard(signal::SIGSEGV)),
      ..
    } => Some(EventStatus::ProcessSegfault),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(Signal::Standard(signal::SIGABRT)),
      ..
    } => Some(EventStatus::ProcessAborted),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(Signal::Standard(signal::SIGILL)),
      ..
    } => Some(EventStatus::ProcessIllegalInstruction),
    ProcessStateUpdate::Exit {
      status: ProcessExit::Signal(s),
      ..
    } => Some(EventStatus::ProcessSignaled(*s)),
    ProcessStateUpdate::BreakPointHit { .. } => Some(EventStatus::ProcessPaused),
    ProcessStateUpdate::Resumed => Some(EventStatus::ProcessRunning),
    ProcessStateUpdate::Detached { .. } => Some(EventStatus::ProcessDetached),
    ProcessStateUpdate::ResumeError { .. } | ProcessStateUpdate::DetachError { .. } => {
      Some(EventStatus::InternalError)
    }
  }
}
