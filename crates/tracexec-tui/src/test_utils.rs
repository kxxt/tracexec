//! Test utilities for snapshot testing with ratatui and insta

use ratatui::{
  Terminal,
  backend::TestBackend,
  layout::Rect,
  widgets::{
    FrameExt,
    StatefulWidgetRef,
    Widget,
  },
};

/// Common terminal sizes for testing
pub mod sizes {
  pub const SMALL: (u16, u16) = (40, 20);
  pub const MEDIUM: (u16, u16) = (80, 24);
  pub const LARGE: (u16, u16) = (120, 40);
  pub const WIDE: (u16, u16) = (160, 40);
}

/// Render a widget to a string for snapshot testing
pub fn test_render_widget<W>(widget: W, width: u16, height: u16) -> String
where
  W: Widget,
{
  let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
  terminal
    .draw(|frame| frame.render_widget(widget, frame.area()))
    .unwrap();
  format!("{:?}", terminal.backend().buffer())
}

/// Render a widget with a specific area for snapshot testing
pub fn test_render_widget_area<W>(widget: W, area: Rect) -> String
where
  W: Widget,
{
  let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
  terminal
    .draw(|frame| frame.render_widget(widget, area))
    .unwrap();
  format!("{:?}", terminal.backend().buffer())
}

/// Render a StatefulWidgetRef with a specific area for snapshot testing
pub fn test_render_stateful_widget_area<W, S>(widget: W, area: Rect, state: &mut S) -> String
where
  W: StatefulWidgetRef<State = S>,
{
  let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
  terminal
    .draw(|frame| frame.render_stateful_widget_ref(widget, area, state))
    .unwrap();
  format!("{:?}", terminal.backend().buffer())
}

/// Create a test area with the given dimensions
pub fn test_area(x: u16, y: u16, width: u16, height: u16) -> Rect {
  Rect::new(x, y, width, height)
}

/// Create a test area that fills the terminal
pub fn test_area_full(width: u16, height: u16) -> Rect {
  test_area(0, 0, width, height)
}
