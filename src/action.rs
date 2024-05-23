use std::sync::Arc;

use crossterm::event::KeyEvent;
use ratatui::layout::Size;

use crate::{
  event::TracerEventDetails,
  tui::{
    copy_popup::CopyPopupState, details_popup::DetailsPopupState, error_popup::ErrorPopupState,
    query::Query,
  },
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
  ToggleCwdDisplay,
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
  ShowCopyDialog(Arc<TracerEventDetails>),
  CopyToClipboard {
    target: CopyTarget,
    event: Arc<TracerEventDetails>,
  },
  // Query
  BeginSearch,
  EndSearch,
  ExecuteSearch(Query),
  NextMatch,
  PrevMatch,
  // Terminal
  HandleTerminalKeyPress(KeyEvent),
  // Breakpoint
  ShowBreakpointManager,
  CloseBreakpointManager,
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
  ErrorPopup(ErrorPopupState),
}
