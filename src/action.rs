use crossterm::event::KeyEvent;
use ratatui::layout::Size;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
  Quit,
  Render,
  Resize(Size),
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
  ShrinkPane,
  GrowPane,
  SwitchLayout,
  SwitchActivePane,
  SetActivePopup(ActivePopup),
  CopyToClipboard(CopyTarget),
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
