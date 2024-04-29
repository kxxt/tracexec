use crossterm::event::KeyEvent;
use ratatui::layout::Size;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
  // Application
  Quit,
  // Rendering
  Render,
  // Resize
  Resize(Size),
  // Navigation
  NextItem,
  PrevItem,
  PageDown,
  PageUp,
  PageLeft,
  PageRight,
  ScrollLeft,
  ScrollRight,
  ScrollToTop,
  ScrollToBottom,
  ScrollToStart,
  ScrollToEnd,
  ToggleFollow,
  // Sizing
  ShrinkPane,
  GrowPane,
  // Layout
  SwitchLayout,
  // Pane
  SwitchActivePane,
  // Popup
  SetActivePopup(ActivePopup),
  // Clipboard
  CopyToClipboard(CopyTarget),
  // Terminal
  HandleTerminalKeyPress(KeyEvent),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopyTarget {
  Commandline(SupportedShell),
  Env,
  Argv,
  Filename,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SupportedShell {
  Bash,
  Sh,
  Fish,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivePopup {
  Help,
  CopyTargetSelection,
}
