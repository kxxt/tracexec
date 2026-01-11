use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use ratatui::text::Line;
use tracexec_core::{
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
    action_tx: &LocalUnboundedSender<Action>,
  ) -> color_eyre::Result<()> {
    match ke.code {
      KeyCode::Down | KeyCode::Char('j') => {
        if ke.modifiers == KeyModifiers::CONTROL {
          action_tx.send(Action::PageDown);
        } else if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::NextItem);
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Up | KeyCode::Char('k') => {
        if ke.modifiers == KeyModifiers::CONTROL {
          action_tx.send(Action::StopFollow);
          action_tx.send(Action::PageUp);
        } else if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::StopFollow);
          action_tx.send(Action::PrevItem);
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Left | KeyCode::Char('h') => {
        if ke.modifiers == KeyModifiers::CONTROL {
          action_tx.send(Action::PageLeft);
        } else if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::ScrollLeft);
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Right | KeyCode::Char('l') if ke.modifiers != KeyModifiers::ALT => {
        if ke.modifiers == KeyModifiers::CONTROL {
          action_tx.send(Action::PageRight);
        } else if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::ScrollRight);
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::PageDown if ke.modifiers == KeyModifiers::NONE => {
        action_tx.send(Action::PageDown);
        // action_tx.send(Action::Render)?;
      }
      KeyCode::PageUp if ke.modifiers == KeyModifiers::NONE => {
        action_tx.send(Action::StopFollow);
        action_tx.send(Action::PageUp);
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Home => {
        if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::StopFollow);
          action_tx.send(Action::ScrollToTop);
        } else if ke.modifiers == KeyModifiers::SHIFT {
          action_tx.send(Action::ScrollToStart);
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::End => {
        if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::ScrollToBottom);
        } else if ke.modifiers == KeyModifiers::SHIFT {
          action_tx.send(Action::ScrollToEnd);
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Char('g') if ke.modifiers == KeyModifiers::NONE => {
        action_tx.send(Action::GrowPane);
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Char('s') => {
        if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::ShrinkPane);
        } else if ke.modifiers == KeyModifiers::ALT {
          action_tx.send(Action::HandleTerminalKeyPress(KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL,
          )));
        }
        // action_tx.send(Action::Render)?;
      }
      KeyCode::Char('c') if ke.modifiers == KeyModifiers::NONE && self.has_clipboard => {
        if let Some(details) = self.selection_map(|e| e.details.clone()) {
          action_tx.send(Action::ShowCopyDialog(details));
        }
      }
      KeyCode::Char('f') if self.is_primary => {
        if ke.modifiers == KeyModifiers::NONE {
          action_tx.send(Action::ToggleFollow);
        } else if ke.modifiers == KeyModifiers::CONTROL {
          action_tx.send(Action::BeginSearch);
        }
      }
      KeyCode::Char('e') if ke.modifiers == KeyModifiers::NONE => {
        action_tx.send(Action::ToggleEnvDisplay);
      }
      KeyCode::Char('w') if ke.modifiers == KeyModifiers::NONE => {
        action_tx.send(Action::ToggleCwdDisplay);
      }
      KeyCode::F(1) if ke.modifiers == KeyModifiers::NONE => {
        action_tx.send(Action::SetActivePopup(ActivePopup::Help));
      }
      KeyCode::Char('v') if ke.modifiers == KeyModifiers::NONE => {
        if let Some(event) = self.selection() {
          action_tx.send(Action::SetActivePopup(ActivePopup::ViewDetails(
            DetailsPopupState::new(&event.borrow(), self),
          )));
        }
      }
      // TODO: implement this for secondary event list
      // Currently we only use secondary event list for displaying backtrace,
      // goto parent is not actually useful in such case.
      // But we should support filtering in the future, which could use this feature.
      // We are missing the id <-> index mapping for non-contiguous event ids to implement it.
      KeyCode::Char('u') if ke.modifiers == KeyModifiers::NONE && self.is_primary => {
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
                )));
              }
            } else {
              action_tx.send(Action::SetActivePopup(err_popup_goto_parent_not_found(
                "Go To Parent Result",
              )));
            }
          } else {
            action_tx.send(Action::SetActivePopup(err_popup_goto_parent_not_exec(
              "Go to Parent Error",
            )));
          }
        }
      }
      KeyCode::Char('b') if ke.modifiers == KeyModifiers::NONE && self.is_ptrace => {
        action_tx.send(Action::ShowBreakpointManager);
      }
      KeyCode::Char('z') if ke.modifiers == KeyModifiers::NONE && self.is_ptrace => {
        action_tx.send(Action::ShowHitManager);
      }
      KeyCode::Char('t') if ke.modifiers == KeyModifiers::NONE && self.is_primary => {
        if let Some(e) = self.selection() {
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
              ),
            )));
          }
        }
      }
      _ => {}
    }
    Ok(())
  }
}
