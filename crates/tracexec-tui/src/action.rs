use std::sync::Arc;

use crossterm::event::KeyEvent;
use ratatui::{
  layout::Size,
  text::Line,
};
use tracexec_core::event::{
  EventId,
  TracerEventDetails,
};

use crate::{
  backtrace_popup::BacktracePopupState,
  copy_popup::CopyPopupState,
  details_popup::DetailsPopupState,
  error_popup::InfoPopupState,
  query::Query,
};

#[derive(Debug)]
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
  ScrollToId(EventId),
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
  ShowHitManager,
  HideHitManager,
}

impl Action {
  pub fn show_error_popup<E: ToString>(title: String, error: E) -> Self {
    Self::SetActivePopup(ActivePopup::InfoPopup(InfoPopupState::error(
      title,
      vec![Line::raw(error.to_string())],
    )))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyTarget {
  Line,
  Commandline(SupportedShell),
  CommandlineWithFullEnv(SupportedShell),
  CommandlineWithStdio(SupportedShell),
  CommandlineWithFds(SupportedShell),
  Env,
  Argv,
  Filename,
  SyscallResult,
  EnvDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedShell {
  Bash,
  Sh,
  Fish,
}

#[derive(Debug)]
pub enum ActivePopup {
  Help,
  Backtrace(Box<BacktracePopupState>),
  ViewDetails(DetailsPopupState),
  CopyTargetSelection(CopyPopupState),
  InfoPopup(InfoPopupState),
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::theme::THEME;

  #[test]
  fn test_show_error_popup_creates_popup() {
    let action = Action::show_error_popup("Test Error".to_string(), "Something went wrong");
    match action {
      Action::SetActivePopup(ActivePopup::InfoPopup(popup_state)) => {
        assert_eq!(popup_state.title, "Test Error");
        assert_eq!(popup_state.style, THEME.error_popup);
        assert_eq!(popup_state.message.len(), 1);
        assert_eq!(
          popup_state.message[0].spans[0].content.as_ref(),
          "Something went wrong"
        );
      }
      _ => panic!("Expected SetActivePopup with InfoPopup"),
    }
  }

  #[test]
  fn test_show_error_popup_with_display_trait() {
    let error_code = 42;
    let action = Action::show_error_popup("Error Code".to_string(), error_code);
    match action {
      Action::SetActivePopup(ActivePopup::InfoPopup(popup_state)) => {
        assert_eq!(popup_state.title, "Error Code");
        assert_eq!(popup_state.style, THEME.error_popup);
        assert_eq!(popup_state.message.len(), 1);
        assert_eq!(popup_state.message[0].spans[0].content.as_ref(), "42");
      }
      _ => panic!("Expected SetActivePopup with InfoPopup"),
    }
  }
}
