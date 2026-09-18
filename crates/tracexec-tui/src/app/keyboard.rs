use crossterm::event::KeyEvent;
use tracexec_core::{
  cli::{
    keys::{
      KeyAction,
      KeyRouter,
    },
    options::ActivePane,
  },
  primitives::local_chan::LocalUnboundedSender,
};
use tracing::trace;

use super::App;
use crate::{
  action::{
    Action,
    ActivePopup,
  },
  error_popup::InfoPopupState,
};

impl App {
  pub(super) async fn route_key_event(
    &mut self,
    event: KeyEvent,
    action_tx: &LocalUnboundedSender<Action>,
  ) -> color_eyre::Result<()> {
    let key_bindings = self.key_bindings.clone();
    let key = KeyRouter::new(&key_bindings, event);

    if key.matches(KeyAction::SwitchPane) {
      self.switch_pane(action_tx).await;
      return Ok(());
    }

    trace!("TUI: Active pane: {}", self.active_pane);
    if self.active_pane == ActivePane::Terminal {
      action_tx.send(Action::HandleTerminalKeyPress(event));
      return Ok(());
    }

    if let Some(popup) = self.popup.last_mut() {
      match popup {
        ActivePopup::Backtrace(state) => {
          state
            .handle_key_event(event, &key_bindings, action_tx)
            .await?;
        }
        ActivePopup::Help => {
          self.popup.pop();
        }
        ActivePopup::ViewDetails(state) => {
          state.handle_key_event(
            event,
            &key_bindings,
            self.clipboard.as_mut(),
            &self.event_list,
            action_tx,
          )?;
        }
        ActivePopup::CopyTargetSelection(state) => {
          if let Some(action) = state.handle_key_event(event)? {
            action_tx.send(action);
          }
        }
        ActivePopup::InfoPopup(state) => {
          if let Some(action) = state.handle_key_event(event) {
            action_tx.send(action);
          }
        }
      }
      return Ok(());
    }

    if let Some(hit_manager) = self.hit_manager_state.as_mut()
      && hit_manager.visible
    {
      if let Some(action) = hit_manager.handle_key_event(event, &key_bindings) {
        action_tx.send(action);
      }
      return Ok(());
    }

    if let Some(breakpoint_manager) = self.breakpoint_manager.as_mut() {
      if let Some(action) = breakpoint_manager.handle_key_event(event, &key_bindings) {
        action_tx.send(action);
      }
      return Ok(());
    }

    if let Some(query_builder) = self.query_builder.as_mut() {
      if query_builder.editing() {
        match query_builder.handle_key_events(event, &key_bindings) {
          Ok(Some(action)) => action_tx.send(action),
          Ok(None) => {}
          Err(error) => self
            .popup
            .push(ActivePopup::InfoPopup(InfoPopupState::error(
              "Regex Error".to_owned(),
              error,
              self.theme,
            ))),
        }
        return Ok(());
      }

      if let Some(action) = key.resolve(&[KeyAction::QueryNextMatch, KeyAction::QueryPrevMatch]) {
        match action {
          KeyAction::QueryNextMatch => {
            trace!("Query: Next match");
            action_tx.send(Action::NextMatch);
          }
          KeyAction::QueryPrevMatch => {
            trace!("Query: Prev match");
            action_tx.send(Action::PrevMatch);
          }
          _ => unreachable!(),
        }
        return Ok(());
      }
    }

    match key.resolve(&[KeyAction::Quit, KeyAction::SwitchLayout]) {
      Some(KeyAction::Quit) => action_tx.send(Action::Quit),
      Some(KeyAction::SwitchLayout) => action_tx.send(Action::SwitchLayout),
      Some(_) => unreachable!(),
      None => {
        self
          .event_list
          .handle_key_event(event, &key_bindings, action_tx)
          .await?;
      }
    }
    Ok(())
  }

  async fn switch_pane(&mut self, action_tx: &LocalUnboundedSender<Action>) {
    action_tx.send(Action::SwitchActivePane);
    self.popup.clear();

    if self
      .query_builder
      .as_ref()
      .is_some_and(|builder| builder.editing())
    {
      self.query_builder = None;
      self.event_list.set_query(None).await;
    }

    self.breakpoint_manager = None;
    if let Some(hit_manager) = self.hit_manager_state.as_mut()
      && hit_manager.visible
    {
      hit_manager.hide();
    }
  }
}
