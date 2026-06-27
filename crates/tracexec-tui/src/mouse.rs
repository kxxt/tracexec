use std::time::Instant;

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;

/// Stores the areas of UI components as computed during rendering.
/// Used for mouse hit-testing to determine which component was clicked.
#[derive(Debug, Default, Clone)]
pub struct LayoutAreas {
  /// Inner area of the event list (content area without the border)
  pub event_list_inner: Rect,
  /// Outer area of the event list (including border)
  pub event_list_outer: Rect,
  /// Inner area of the terminal pane (without border)
  pub terminal_inner: Option<Rect>,
  /// Outer area of the terminal pane (including border)
  pub terminal_outer: Option<Rect>,
  /// Footer (help bar) area
  pub footer: Rect,
  /// Title-bar help entries (e.g. in breakpoint manager, hit manager)
  pub title_bar_entries: Vec<HelpBarEntry>,
  /// The rest area containing both event list and terminal pane, used for divider dragging
  pub rest_area: Rect,
}

/// A clickable region in the help bar.
#[derive(Debug, Clone)]
pub struct HelpBarEntry {
  /// The screen area of this clickable region.
  pub area: Rect,
  /// The key event to simulate when this region is clicked.
  pub key_event: KeyEvent,
}

/// Tracks click state for double-click detection.
#[derive(Debug, Default)]
pub struct ClickTracker {
  last_click: Option<(u16, u16, Instant)>,
}

impl ClickTracker {
  const DOUBLE_CLICK_THRESHOLD_MS: u128 = 500;

  /// Record a click and return whether it constitutes a double-click.
  pub fn record_click(&mut self, col: u16, row: u16) -> bool {
    let now = Instant::now();
    let is_double = self
      .last_click
      .as_ref()
      .map(|(lc, lr, lt)| {
        *lc == col
          && *lr == row
          && now.duration_since(*lt).as_millis() < Self::DOUBLE_CLICK_THRESHOLD_MS
      })
      .unwrap_or(false);

    if is_double {
      self.last_click = None;
    } else {
      self.last_click = Some((col, row, now));
    }

    is_double
  }
}

/// Check if a screen position is within a given rectangle.
pub fn position_in_rect(col: u16, row: u16, rect: &Rect) -> bool {
  col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Tracks the current mouse cursor position for hover effects.
#[derive(Debug, Default, Clone, Copy)]
pub struct HoverState {
  pub col: u16,
  pub row: u16,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn click_tracker_single_click() {
    let mut tracker = ClickTracker::default();
    assert!(!tracker.record_click(5, 10));
  }

  #[test]
  fn click_tracker_double_click() {
    let mut tracker = ClickTracker::default();
    assert!(!tracker.record_click(5, 10));
    assert!(tracker.record_click(5, 10));
  }

  #[test]
  fn click_tracker_different_position_not_double() {
    let mut tracker = ClickTracker::default();
    assert!(!tracker.record_click(5, 10));
    assert!(!tracker.record_click(6, 10));
  }

  #[test]
  fn click_tracker_resets_after_double_click() {
    let mut tracker = ClickTracker::default();
    assert!(!tracker.record_click(5, 10));
    assert!(tracker.record_click(5, 10));
    // After double-click, next click should be single
    assert!(!tracker.record_click(5, 10));
  }

  #[test]
  fn position_in_rect_inside() {
    let rect = Rect::new(10, 20, 30, 5);
    assert!(position_in_rect(10, 20, &rect));
    assert!(position_in_rect(39, 24, &rect));
    assert!(position_in_rect(25, 22, &rect));
  }

  #[test]
  fn position_in_rect_outside() {
    let rect = Rect::new(10, 20, 30, 5);
    assert!(!position_in_rect(9, 20, &rect));
    assert!(!position_in_rect(40, 20, &rect));
    assert!(!position_in_rect(10, 19, &rect));
    assert!(!position_in_rect(10, 25, &rect));
  }

  #[test]
  fn position_in_rect_empty() {
    let rect = Rect::new(0, 0, 0, 0);
    assert!(!position_in_rect(0, 0, &rect));
  }
}
