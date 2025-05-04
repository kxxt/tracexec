// Copyright (c) 2023 Ratatui Developers
// Copyright (c) 2024 Levi Zim

// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
// associated documentation files (the "Software"), to deal in the Software without restriction,
// including without limitation the rights to use, copy, modify, merge, publish, distribute,
// sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all copies or substantial
// portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
// NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
// OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::{ops::ControlFlow, sync::Arc};

use arboard::Clipboard;
use clap::ValueEnum;
use crossterm::event::{KeyCode, KeyModifiers};

use nix::{errno::Errno, sys::signal::Signal, unistd::Pid};
use ratatui::{layout::Position, style::Stylize, text::Line, widgets::Widget};
use serde::{Deserialize, Serialize};
use strum::Display;
use tokio::sync::mpsc;
use tracing::{debug, trace};

use crate::{
  action::{Action, ActivePopup},
  cli::{
    args::{DebuggerArgs, LogModeArgs, ModifierArgs, TuiModeArgs},
    config::ExitHandling,
    options::ActivePane,
  },
  event::{Event, ProcessStateUpdate, ProcessStateUpdateEvent, TracerEventDetails, TracerMessage},
  printer::PrinterArgs,
  proc::BaselineInfo,
  ptrace::Tracer,
  pty::{PtySize, UnixMasterPty},
  tui::{error_popup::InfoPopupState, query::QueryKind},
};

use super::{
  Tui, breakpoint_manager::BreakPointManagerState, copy_popup::CopyPopupState,
  event_list::EventList, hit_manager::HitManagerState, pseudo_term::PseudoTerminalPane,
  query::QueryBuilder,
};

mod ui;

pub const DEFAULT_MAX_EVENTS: u64 = 1_000_000;

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Display, Deserialize, Serialize,
)]
#[strum(serialize_all = "kebab-case")]
pub enum AppLayout {
  #[default]
  Horizontal,
  Vertical,
}

pub struct App {
  pub event_list: EventList,
  pub printer_args: PrinterArgs,
  pub term: Option<PseudoTerminalPane>,
  pub root_pid: Option<Pid>,
  pub active_pane: ActivePane,
  pub clipboard: Option<Clipboard>,
  pub split_percentage: u16,
  pub layout: AppLayout,
  pub should_handle_internal_resize: bool,
  pub popup: Vec<ActivePopup>,
  pub active_experiments: Vec<&'static str>,
  tracer: Option<Arc<Tracer>>,
  query_builder: Option<QueryBuilder>,
  breakpoint_manager: Option<BreakPointManagerState>,
  hit_manager_state: Option<HitManagerState>,
  exit_handling: ExitHandling,
}

pub struct PTracer {
  pub tracer: Arc<Tracer>,
  pub debugger_args: DebuggerArgs,
}

impl App {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    mut tracer: Option<PTracer>,
    tracing_args: &LogModeArgs,
    modifier_args: &ModifierArgs,
    tui_args: TuiModeArgs,
    baseline: Arc<BaselineInfo>,
    pty_master: Option<UnixMasterPty>,
  ) -> color_eyre::Result<Self> {
    let active_pane = if pty_master.is_some() {
      tui_args.active_pane.unwrap_or_default()
    } else {
      ActivePane::Events
    };
    if let Some(tracer) = tracer.as_mut() {
      for bp in tracer.debugger_args.breakpoints.drain(..) {
        tracer.tracer.add_breakpoint(bp);
      }
    }
    let clipboard = Clipboard::new().ok();
    Ok(Self {
      event_list: EventList::new(
        baseline,
        tui_args.follow,
        modifier_args.to_owned(),
        tui_args.max_events.unwrap_or(DEFAULT_MAX_EVENTS),
        tracer.is_some(),
        clipboard.is_some(),
      ),
      printer_args: PrinterArgs::from_cli(tracing_args, modifier_args),
      split_percentage: if pty_master.is_some() { 50 } else { 100 },
      term: if let Some(pty_master) = pty_master {
        let mut term = PseudoTerminalPane::new(
          PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
          },
          pty_master,
        )?;
        if active_pane == ActivePane::Terminal {
          term.focus(true);
        }
        Some(term)
      } else {
        None
      },
      root_pid: None,
      active_pane,
      clipboard,
      layout: tui_args.layout.unwrap_or_default(),
      should_handle_internal_resize: true,
      popup: vec![],
      query_builder: None,
      breakpoint_manager: None,
      active_experiments: vec![],
      tracer: tracer.as_ref().map(|t| t.tracer.clone()),
      hit_manager_state: tracer
        .map(|t| HitManagerState::new(t.tracer, t.debugger_args.default_external_command))
        .transpose()?,
      exit_handling: {
        if tui_args.kill_on_exit {
          ExitHandling::Kill
        } else if tui_args.terminate_on_exit {
          ExitHandling::Terminate
        } else {
          ExitHandling::Wait
        }
      },
    })
  }

  pub fn activate_experiment(&mut self, experiment: &'static str) {
    self.active_experiments.push(experiment);
  }

  pub fn shrink_pane(&mut self) {
    if self.term.is_some() {
      self.split_percentage = self.split_percentage.saturating_sub(1).max(10);
    }
  }

  pub fn grow_pane(&mut self) {
    if self.term.is_some() {
      self.split_percentage = self.split_percentage.saturating_add(1).min(90);
    }
  }

  pub async fn run(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    loop {
      // Handle events
      if let Some(e) = tui.next().await {
        if e != Event::Render {
          trace!("Received event {e:?}");
        }
        match e {
          Event::ShouldQuit => {
            action_tx.send(Action::Quit)?;
          }
          Event::Key(ke) => {
            if ke.code == KeyCode::Char('s') && ke.modifiers.contains(KeyModifiers::CONTROL) {
              action_tx.send(Action::SwitchActivePane)?;
              // Cancel all popups
              self.popup.clear();
              // Cancel non-finished query
              if self.query_builder.as_ref().is_some_and(|b| b.editing()) {
                self.query_builder = None;
                self.event_list.set_query(None).await;
              }
              // Cancel breakpoint manager
              if self.breakpoint_manager.is_some() {
                self.breakpoint_manager = None;
              }
              // Cancel hit manager
              if let Some(h) = self.hit_manager_state.as_mut() {
                if h.visible {
                  h.hide();
                }
              }
              // action_tx.send(Action::Render)?;
            } else {
              trace!("TUI: Active pane: {}", self.active_pane);
              if self.active_pane == ActivePane::Events {
                // Handle popups
                // TODO: do this in a separate function
                if let Some(popup) = &mut self.popup.last_mut() {
                  match popup {
                    ActivePopup::Backtrace(state) => state.handle_key_event(ke, &action_tx).await?,
                    ActivePopup::Help => {
                      self.popup.pop();
                    }
                    ActivePopup::ViewDetails(state) => {
                      if ControlFlow::Break(())
                        == state.handle_key_event(ke, self.clipboard.as_mut())?
                      {
                        self.popup.pop();
                      }
                    }
                    ActivePopup::CopyTargetSelection(state) => {
                      if let Some(action) = state.handle_key_event(ke)? {
                        action_tx.send(action)?;
                      }
                    }
                    ActivePopup::InfoPopup(state) => {
                      if let Some(action) = state.handle_key_event(ke) {
                        action_tx.send(action)?;
                      }
                    }
                  }
                  continue;
                }

                // Handle hit manager
                if let Some(h) = self.hit_manager_state.as_mut() {
                  if h.visible {
                    if let Some(action) = h.handle_key_event(ke) {
                      action_tx.send(action)?;
                    }
                    continue;
                  }
                }

                // Handle breakpoint manager
                if let Some(breakpoint_manager) = self.breakpoint_manager.as_mut() {
                  if let Some(action) = breakpoint_manager.handle_key_event(ke) {
                    action_tx.send(action)?;
                  }
                  continue;
                }

                // Handle query builder
                if let Some(query_builder) = self.query_builder.as_mut() {
                  if query_builder.editing() {
                    match query_builder.handle_key_events(ke) {
                      Ok(result) => {
                        result.map(|action| action_tx.send(action)).transpose()?;
                      }
                      Err(e) => {
                        // Regex error
                        self
                          .popup
                          .push(ActivePopup::InfoPopup(InfoPopupState::error(
                            "Regex Error".to_owned(),
                            e,
                          )));
                      }
                    }
                    continue;
                  } else {
                    match (ke.code, ke.modifiers) {
                      (KeyCode::Char('n'), KeyModifiers::NONE) => {
                        trace!("Query: Next match");
                        action_tx.send(Action::NextMatch)?;
                        continue;
                      }
                      (KeyCode::Char('p'), KeyModifiers::NONE) => {
                        trace!("Query: Prev match");
                        action_tx.send(Action::PrevMatch)?;
                        continue;
                      }
                      _ => {}
                    }
                  }
                }

                match ke.code {
                  KeyCode::Char('q') if ke.modifiers == KeyModifiers::NONE => {
                    if !self.popup.is_empty() {
                      self.popup.pop();
                    } else {
                      action_tx.send(Action::Quit)?;
                    }
                  }
                  _ => self.event_list.handle_key_event(ke, &action_tx).await?,
                }
              } else {
                action_tx.send(Action::HandleTerminalKeyPress(ke))?;
                // action_tx.send(Action::Render)?;
              }
            }
          }
          Event::Tracer(msg) => {
            match msg {
              TracerMessage::Event(e) => {
                if let TracerEventDetails::TraceeSpawn { pid, .. } = &e.details {
                  // FIXME: we should not rely on TracerMessage, which might be filtered.
                  debug!("Received tracee spawn event: {pid}");
                  self.root_pid = Some(*pid);
                }
                self.event_list.push(e.id, e.details).await;
                if self.event_list.is_following() {
                  action_tx.send(Action::ScrollToBottom)?;
                }
              }
              TracerMessage::StateUpdate(update) => {
                trace!("Received process state update: {update:?}");
                let mut handled = false;
                match &update {
                  ProcessStateUpdateEvent {
                    update: ProcessStateUpdate::BreakPointHit(hit),
                    ..
                  } => {
                    self
                      .hit_manager_state
                      .access_some_mut(|h| _ = h.add_hit(*hit));
                    // Warn: This grants CAP_SYS_ADMIN to not only the tracer but also the tracees
                    // sudo -E env RUST_LOG=debug setpriv --reuid=$(id -u) --regid=$(id -g) --init-groups --inh-caps=+sys_admin --ambient-caps +sys_admin -- target/debug/tracexec tui -t --
                  }
                  ProcessStateUpdateEvent {
                    update: ProcessStateUpdate::Detached { hid, .. },
                    pid,
                    ..
                  } => {
                    if let Some(Err(e)) = self
                      .hit_manager_state
                      .as_mut()
                      .map(|h| h.react_on_process_detach(*hid, *pid))
                    {
                      action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
                        InfoPopupState::error(
                          "Detach Error".to_owned(),
                          vec![
                            Line::default().spans(vec![
                              "Failed to run custom command after detaching process ".into(),
                              pid.to_string().bold(),
                              ". Error: ".into(),
                            ]),
                            e.to_string().into(),
                          ],
                        ),
                      )))?;
                    }
                  }
                  ProcessStateUpdateEvent {
                    update: ProcessStateUpdate::ResumeError { hit, error },
                    ..
                  } => {
                    if *error != Errno::ESRCH {
                      self
                        .hit_manager_state
                        .access_some_mut(|h| _ = h.add_hit(*hit));
                    }
                    action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
                      InfoPopupState::error(
                        "Resume Error".to_owned(),
                        vec![
                          Line::default().spans(vec![
                            "Failed to resume process ".into(),
                            hit.pid.to_string().bold(),
                            ". Error: ".into(),
                          ]),
                          error.to_string().into(),
                        ],
                      ),
                    )))?;
                    handled = true;
                  }
                  ProcessStateUpdateEvent {
                    update: ProcessStateUpdate::DetachError { hit, error },
                    ..
                  } => {
                    if *error != Errno::ESRCH {
                      self
                        .hit_manager_state
                        .access_some_mut(|h| _ = h.add_hit(*hit));
                    }
                    action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
                      InfoPopupState::error(
                        "Detach Error".to_owned(),
                        vec![
                          Line::default().spans(vec![
                            "Failed to detach process ".into(),
                            hit.pid.to_string().bold(),
                            ". Error: ".into(),
                          ]),
                          error.to_string().into(),
                        ],
                      ),
                    )))?;
                    handled = true;
                  }
                  _ => (),
                }
                if !handled {
                  self.event_list.update(update).await;
                }
              }
              TracerMessage::FatalError(e) => {
                action_tx.send(Action::SetActivePopup(ActivePopup::InfoPopup(
                  InfoPopupState::error(
                    "FATAL ERROR in tracer thread".to_string(),
                    vec![
                      Line::raw("The tracer thread has died abnormally! error: "),
                      e.into(),
                    ],
                  ),
                )))?;
              }
            }
            // action_tx.send(Action::Render)?;
          }
          Event::Render => {
            action_tx.send(Action::Render)?;
          }
          Event::Resize(size) => {
            action_tx.send(Action::Resize(size))?;
            // action_tx.send(Action::Render)?;
          }
          Event::Init => {
            // Fix the size of the terminal
            action_tx.send(Action::Resize(tui.size()?))?;
            // action_tx.send(Action::Render)?;
          }
          Event::Error => {}
        }
      }

      // Handle actions
      while let Ok(action) = action_rx.try_recv() {
        if !matches!(action, Action::Render) {
          debug!("action: {action:?}");
        }
        match action {
          Action::Quit => {
            return Ok(());
          }
          Action::Render => {
            tui.draw(|f| {
              self.render(f.area(), f.buffer_mut());
              self
                .query_builder
                .as_ref()
                .filter(|q| q.editing())
                .inspect(|q| {
                  let (x, y) = q.cursor();
                  f.set_cursor_position(Position::new(x, y));
                });
              if let Some((x, y)) = self
                .breakpoint_manager
                .as_ref()
                .and_then(|mgr| mgr.cursor())
              {
                f.set_cursor_position(Position::new(x, y));
              }
              if let Some((x, y)) = self.hit_manager_state.as_ref().and_then(|x| x.cursor()) {
                f.set_cursor_position(Position::new(x, y));
              }
            })?;
          }
          Action::NextItem => {
            self.event_list.next();
          }
          Action::PrevItem => {
            self.event_list.previous();
          }
          Action::PageDown => {
            self.event_list.page_down();
          }
          Action::PageUp => {
            self.event_list.page_up();
          }
          Action::PageLeft => {
            self.event_list.page_left();
          }
          Action::PageRight => {
            self.event_list.page_right();
          }
          Action::HandleTerminalKeyPress(ke) => {
            if let Some(term) = self.term.as_mut() {
              term.handle_key_event(&ke).await;
            }
          }
          Action::Resize(_size) => {
            self.should_handle_internal_resize = true;
          }
          Action::ScrollLeft => {
            self.event_list.scroll_left();
          }
          Action::ScrollRight => {
            self.event_list.scroll_right();
          }
          Action::ScrollToTop => {
            self.event_list.scroll_to_top();
          }
          Action::ScrollToBottom => {
            self.event_list.scroll_to_bottom();
          }
          Action::ScrollToStart => {
            self.event_list.scroll_to_start();
          }
          Action::ScrollToEnd => {
            self.event_list.scroll_to_end();
          }
          Action::ScrollToId(id) => {
            self.event_list.scroll_to_id(Some(id)).await;
          }
          Action::ToggleFollow => {
            self.event_list.toggle_follow();
            if self.event_list.is_following() {
              action_tx.send(Action::ScrollToBottom)?;
            }
          }
          Action::ToggleEnvDisplay => {
            self.event_list.toggle_env_display().await;
          }
          Action::ToggleCwdDisplay => {
            self.event_list.toggle_cwd_display().await;
          }
          Action::StopFollow => {
            self.event_list.stop_follow();
          }
          Action::ShrinkPane => {
            self.shrink_pane();
            self.should_handle_internal_resize = true;
          }
          Action::GrowPane => {
            self.grow_pane();
            self.should_handle_internal_resize = true;
          }
          Action::SwitchLayout => {
            self.layout = match self.layout {
              AppLayout::Horizontal => AppLayout::Vertical,
              AppLayout::Vertical => AppLayout::Horizontal,
            };
            self.should_handle_internal_resize = true;
          }
          Action::SwitchActivePane => {
            self.active_pane = match self.active_pane {
              ActivePane::Events => {
                if let Some(term) = self.term.as_mut() {
                  term.focus(true);
                  ActivePane::Terminal
                } else {
                  if let Some(t) = self.term.as_mut() {
                    t.focus(false)
                  }
                  ActivePane::Events
                }
              }
              ActivePane::Terminal => {
                if let Some(t) = self.term.as_mut() {
                  t.focus(false)
                }
                ActivePane::Events
              }
            }
          }
          Action::ShowCopyDialog(e) => {
            self
              .popup
              .push(ActivePopup::CopyTargetSelection(CopyPopupState::new(e)));
          }
          Action::CopyToClipboard { event, target } => {
            let text = event.text_for_copy(
              &self.event_list.baseline,
              target,
              &self.event_list.modifier_args,
              self.event_list.runtime_modifier(),
            );
            // TODO: don't crash the app if clipboard fails
            if let Some(clipboard) = self.clipboard.as_mut() {
              clipboard.set_text(text)?;
            }
            // TODO: find a better way to do this
            self.popup.pop();
          }
          Action::SetActivePopup(popup) => {
            self.popup.push(popup);
          }
          Action::CancelCurrentPopup => {
            self.popup.pop();
          }
          Action::BeginSearch => {
            if let Some(query_builder) = self.query_builder.as_mut() {
              // action_tx.send(query_builder.edit())?;
              query_builder.edit();
            } else {
              let mut query_builder = QueryBuilder::new(QueryKind::Search);
              // action_tx.send(query_builder.edit())?;
              query_builder.edit();
              self.query_builder = Some(query_builder);
            }
          }
          Action::EndSearch => {
            self.query_builder = None;
            self.event_list.set_query(None).await;
          }
          Action::ExecuteSearch(query) => {
            self.event_list.set_query(Some(query)).await;
          }
          Action::NextMatch => {
            self.event_list.next_match().await;
          }
          Action::PrevMatch => {
            self.event_list.prev_match().await;
          }
          Action::ShowBreakpointManager => {
            if self.breakpoint_manager.is_none() {
              self.breakpoint_manager = Some(BreakPointManagerState::new(
                self
                  .tracer
                  .as_ref()
                  .expect("BreakPointManager doesn't work without PTracer!")
                  .clone(),
              ));
            }
          }
          Action::CloseBreakpointManager => {
            self.breakpoint_manager = None;
          }
          Action::ShowHitManager => {
            self.hit_manager_state.access_some_mut(|h| h.visible = true);
          }
          Action::HideHitManager => {
            self
              .hit_manager_state
              .access_some_mut(|h| h.visible = false);
          }
        }
      }
    }
  }

  pub fn exit(&self) -> color_eyre::Result<()> {
    // Close pty master
    self.term.as_ref().inspect(|t| t.exit());
    // Terminate root process
    match self.exit_handling {
      ExitHandling::Kill => self.signal_root_process(Signal::SIGKILL)?,
      ExitHandling::Terminate => self.signal_root_process(Signal::SIGTERM)?,
      ExitHandling::Wait => (),
    }
    Ok(())
  }

  pub fn signal_root_process(&self, sig: Signal) -> color_eyre::Result<()> {
    if let Some(root_pid) = self.root_pid {
      nix::sys::signal::kill(root_pid, sig)?;
    }
    Ok(())
  }
}

trait OptionalAccessMut<T> {
  fn access_some_mut(&mut self, f: impl FnOnce(&mut T));
}

impl<T> OptionalAccessMut<T> for Option<T> {
  fn access_some_mut(&mut self, f: impl FnOnce(&mut T)) {
    if let Some(v) = self.as_mut() {
      f(v)
    }
  }
}
