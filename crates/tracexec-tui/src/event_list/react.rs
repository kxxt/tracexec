use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use ratatui::text::Line;
use tracexec_core::{
  cli::keys::TuiKeyBindings,
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
    if keys.page_down.matches(ke) {
      action_tx.send(Action::PageDown);
    } else if keys.next_item.matches(ke) {
      action_tx.send(Action::NextItem);
    } else if keys.page_up.matches(ke) {
      action_tx.send(Action::StopFollow);
      action_tx.send(Action::PageUp);
    } else if keys.prev_item.matches(ke) {
      action_tx.send(Action::StopFollow);
      action_tx.send(Action::PrevItem);
    } else if keys.page_left.matches(ke) {
      action_tx.send(Action::PageLeft);
    } else if keys.scroll_left.matches(ke) {
      action_tx.send(Action::ScrollLeft);
    } else if keys.page_right.matches(ke) {
      action_tx.send(Action::PageRight);
    } else if keys.scroll_right.matches(ke) {
      action_tx.send(Action::ScrollRight);
    } else if keys.scroll_top.matches(ke) {
      action_tx.send(Action::StopFollow);
      action_tx.send(Action::ScrollToTop);
    } else if keys.scroll_start.matches(ke) {
      action_tx.send(Action::ScrollToStart);
    } else if keys.scroll_bottom.matches(ke) {
      action_tx.send(Action::ScrollToBottom);
    } else if keys.scroll_end.matches(ke) {
      action_tx.send(Action::ScrollToEnd);
    } else if keys.event_grow_pane.matches(ke) {
      action_tx.send(Action::GrowPane);
    } else if keys.event_shrink_pane.matches(ke) {
      action_tx.send(Action::ShrinkPane);
    } else if keys.event_send_ctrl_s.matches(ke) {
      action_tx.send(Action::HandleTerminalKeyPress(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
      )));
    } else if keys.event_copy.matches(ke) && self.has_clipboard {
      if let Some(details) = self.selection_map(|e| e.details.clone()) {
        action_tx.send(Action::ShowCopyDialog(details));
      }
    } else if self.is_primary && keys.event_toggle_follow.matches(ke) {
      action_tx.send(Action::ToggleFollow);
    } else if self.is_primary && keys.event_search.matches(ke) {
      action_tx.send(Action::BeginSearch);
    } else if keys.event_toggle_env.matches(ke) {
      action_tx.send(Action::ToggleEnvDisplay);
    } else if keys.event_toggle_cwd.matches(ke) {
      action_tx.send(Action::ToggleCwdDisplay);
    } else if keys.help.matches(ke) {
      action_tx.send(Action::SetActivePopup(ActivePopup::Help));
    } else if keys.event_view_details.matches(ke) {
      if let Some(event) = self.selection() {
        action_tx.send(Action::SetActivePopup(ActivePopup::ViewDetails(
          DetailsPopupState::new(&event.borrow(), self),
        )));
      }
    } else if self.is_primary && keys.event_go_to_parent.matches(ke) {
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
    } else if self.is_ptrace && keys.event_breakpoints.matches(ke) {
      action_tx.send(Action::ShowBreakpointManager);
    } else if self.is_ptrace && keys.event_hits.matches(ke) {
      action_tx.send(Action::ShowHitManager);
    } else if self.is_primary
      && keys.event_backtrace.matches(ke)
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
