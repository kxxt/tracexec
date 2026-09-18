use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use ratatui::text::Line;
use tracexec_core::{
  cli::keys::{
    KeyAction,
    KeyRouter,
    TuiKeyBindings,
  },
  event::TracerEventDetails,
  primitives::local_chan::LocalUnboundedSender,
};

use super::EventList;
use crate::{
  action::{
    Action,
    ActivePopup,
  },
  backtrace_popup::BacktracePopupState,
  details_popup::DetailsPopupState,
  error_popup::{
    InfoPopupState,
    err_popup_goto_parent_miss,
    err_popup_goto_parent_not_exec,
    err_popup_goto_parent_not_found,
  },
};

impl EventList {
  pub async fn handle_key_event(
    &self,
    ke: KeyEvent,
    keys: &TuiKeyBindings,
    action_tx: &LocalUnboundedSender<Action>,
  ) -> color_eyre::Result<()> {
    let key = KeyRouter::new(keys, ke);
    if key.matches(KeyAction::PageDown) {
      action_tx.send(Action::PageDown);
    } else if key.matches(KeyAction::NextItem) {
      action_tx.send(Action::NextItem);
    } else if key.matches(KeyAction::PageUp) {
      action_tx.send(Action::StopFollow);
      action_tx.send(Action::PageUp);
    } else if key.matches(KeyAction::PrevItem) {
      action_tx.send(Action::StopFollow);
      action_tx.send(Action::PrevItem);
    } else if key.matches(KeyAction::PageLeft) {
      action_tx.send(Action::PageLeft);
    } else if key.matches(KeyAction::ScrollLeft) {
      action_tx.send(Action::ScrollLeft);
    } else if key.matches(KeyAction::PageRight) {
      action_tx.send(Action::PageRight);
    } else if key.matches(KeyAction::ScrollRight) {
      action_tx.send(Action::ScrollRight);
    } else if key.matches(KeyAction::ScrollTop) {
      action_tx.send(Action::StopFollow);
      action_tx.send(Action::ScrollToTop);
    } else if key.matches(KeyAction::ScrollStart) {
      action_tx.send(Action::ScrollToStart);
    } else if key.matches(KeyAction::ScrollBottom) {
      action_tx.send(Action::ScrollToBottom);
    } else if key.matches(KeyAction::ScrollEnd) {
      action_tx.send(Action::ScrollToEnd);
    } else if key.matches(KeyAction::EventGrowPane) {
      action_tx.send(Action::GrowPane);
    } else if key.matches(KeyAction::EventShrinkPane) {
      action_tx.send(Action::ShrinkPane);
    } else if key.matches(KeyAction::EventSendCtrlS) {
      action_tx.send(Action::HandleTerminalKeyPress(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
      )));
    } else if key.matches(KeyAction::EventCopy) && self.has_clipboard {
      if let Some(details) = self.selection_map(|e| e.details.clone()) {
        action_tx.send(Action::ShowCopyDialog(details));
      }
    } else if self.is_primary && key.matches(KeyAction::EventToggleFollow) {
      action_tx.send(Action::ToggleFollow);
    } else if self.is_primary && key.matches(KeyAction::EventSearch) {
      action_tx.send(Action::BeginSearch);
    } else if key.matches(KeyAction::EventToggleEnv) {
      action_tx.send(Action::ToggleEnvDisplay);
    } else if key.matches(KeyAction::EventToggleCwd) {
      action_tx.send(Action::ToggleCwdDisplay);
    } else if key.matches(KeyAction::Help) {
      action_tx.send(Action::SetActivePopup(ActivePopup::Help));
    } else if key.matches(KeyAction::EventViewDetails) {
      if let Some(event) = self.selection() {
        action_tx.send(Action::SetActivePopup(ActivePopup::ViewDetails(
          DetailsPopupState::new(&event.borrow(), self),
        )));
      }
    } else if self.is_primary && key.matches(KeyAction::EventGoToParent) {
      // TODO: implement this for secondary event list
      // Currently we only use secondary event list for displaying backtrace,
      // goto parent is not actually useful in such case.
      // But we should support filtering in the future, which could use this feature.
      // We are missing the id <-> index mapping for non-contiguous event ids to implement it.
      if let Some(event) = self.selection() {
        let e = event.borrow();
        if let TracerEventDetails::Exec(exec) = e.details.as_ref() {
          if let Some(parent) = exec.parent {
            let id = parent.into();
            if self.contains(id) {
              action_tx.send(Action::ScrollToId(id));
            } else {
              action_tx.send(Action::SetActivePopup(err_popup_goto_parent_miss(
                "Go To Parent Error",
                self.theme,
              )));
            }
          } else {
            action_tx.send(Action::SetActivePopup(err_popup_goto_parent_not_found(
              "Go To Parent Result",
              self.theme,
            )));
          }
        } else {
          action_tx.send(Action::SetActivePopup(err_popup_goto_parent_not_exec(
            "Go to Parent Error",
            self.theme,
          )));
        }
      }
    } else if self.is_ptrace && key.matches(KeyAction::EventBreakpoints) {
      action_tx.send(Action::ShowBreakpointManager);
    } else if self.is_ptrace && key.matches(KeyAction::EventHits) {
      action_tx.send(Action::ShowHitManager);
    } else if self.is_primary
      && key.matches(KeyAction::EventBacktrace)
      && let Some(e) = self.selection()
    {
      let event = e.borrow();
      if let TracerEventDetails::Exec(_) = event.details.as_ref() {
        drop(event);
        action_tx.send(Action::SetActivePopup(ActivePopup::Backtrace(Box::new(
          BacktracePopupState::new(e, self),
        ))));
      } else {
        action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
          InfoPopupState::info(
            "Backtrace Error".into(),
            vec![Line::raw(
              "Backtrace feature is currently limited to exec events.",
            )],
            self.theme,
          ),
        )));
      }
    }
    Ok(())
  }
}
