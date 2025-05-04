use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;

use crate::{
  action::{Action, ActivePopup},
  event::TracerEventDetails,
  primitives::local_chan::LocalUnboundedSender,
  tui::{
    backtrace_popup::BacktracePopupState, details_popup::DetailsPopupState,
    error_popup::InfoPopupState,
  },
};

use super::EventList;

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
        if let Some(details) = self.selection(|e| e.details.clone()).await {
          action_tx.send(Action::ShowCopyDialog(details));
        }
      }
      KeyCode::Char('l') if ke.modifiers == KeyModifiers::ALT => {
        action_tx.send(Action::SwitchLayout);
      }
      KeyCode::Char('f') => {
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
        if let Some(popup) = self
          .selection(|e| {
            ActivePopup::ViewDetails(DetailsPopupState::new(
              e.details.clone(),
              e.status,
              e.elapsed,
              self.baseline.clone(),
              self.modifier_args.hide_cloexec_fds,
            ))
          })
          .await
        {
          action_tx.send(Action::SetActivePopup(popup));
        }
      }
      KeyCode::Char('u') if ke.modifiers == KeyModifiers::NONE => {
        if let Some(event) = self.selection(|e| e.details.clone()).await {
          if let TracerEventDetails::Exec(exec) = event.as_ref() {
            if let Some(parent) = exec.parent {
              let id = parent.into();
              if self.contains(id) {
                action_tx.send(Action::ScrollToId(id));
              } else {
                action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
                  InfoPopupState::info(
                    "GoTo Parent Result".into(),
                    vec![Line::raw(
                      "The parent exec event is found, but has been cleared from memory.",
                    )],
                  ),
                )));
              }
            } else {
              action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
                InfoPopupState::info(
                  "GoTo Parent Result".into(),
                  vec![Line::raw("No parent exec event is found for this event.")],
                ),
              )));
            }
          } else {
            action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
              InfoPopupState::error(
                "GoTo Parent Error".into(),
                vec![Line::raw(
                  "This feature is currently limited to exec events.",
                )],
              ),
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
      KeyCode::Char('t') if ke.modifiers == KeyModifiers::NONE => {
        if let Some(e) = self.selection(|e| e.clone()).await {
          if let TracerEventDetails::Exec(_) = e.details.as_ref() {
            action_tx.send(Action::SetActivePopup(ActivePopup::Backtrace(
              BacktracePopupState::new(e, self).await,
            )));
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
