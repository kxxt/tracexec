use std::sync::Arc;

use crossterm::event::KeyEvent;
use ratatui::layout::Size;

use crate::{
  event::TracerEvent,
  tui::{copy_popup::CopyPopupState, details_popup::DetailsPopupState, query::Query},
};

#[derive(Debug, Clone)]
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
  ToggleEnvDisplay,
  StopFollow,
  // Sizing
  ShrinkPane,
  GrowPane,
  // Layout
  SwitchLayout,
  // Pane
  SwitchActivePane,
  // Popup
  SetActivePopup(ActivePopup),
  CancelCurrentPopup,
  // Clipboard
  ShowCopyDialog(Arc<TracerEvent>),
  CopyToClipboard {
    target: CopyTarget,
    event: Arc<TracerEvent>,
  },
  // Query
  BeginSearch,
  ExecuteSearch(Query),
  // Terminal
  HandleTerminalKeyPress(KeyEvent),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopyTarget {
  Line,
  Commandline(SupportedShell),
  CommandlineWithStdio(SupportedShell),
  CommandlineWithFds(SupportedShell),
  Env,
  Argv,
  Filename,
  SyscallResult,
  EnvDiff,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SupportedShell {
  Bash,
  Sh,
  Fish,
}

#[derive(Debug, Clone)]
pub enum ActivePopup {
  Help,
  ViewDetails(DetailsPopupState),
  CopyTargetSelection(CopyPopupState),
}
