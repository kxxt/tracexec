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

use crossterm::event::KeyCode;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
  buffer::Buffer,
  layout::{Constraint, Layout, Rect},
  style::{Color, Modifier, Style},
  widgets::{Block, HighlightSpacing, List, ListItem, StatefulWidget, Widget},
};
use tokio::sync::mpsc;


use crate::{
  cli::{
    args::{ModifierArgs, TracingArgs},
    options::ActivePane,
  },
  event::{Action, Event, TracerEvent},
  printer::PrinterArgs,
  pty::{PtySize, UnixMasterPty},
};

use super::{
  event_list::EventList,
  partial_line::PartialLine,
  pseudo_term::PseudoTerminalPane,
  ui::{render_footer, render_title},
  Tui,
};

pub struct App {
  pub event_list: EventList,
  pub printer_args: PrinterArgs,
  pub term: Option<PseudoTerminalPane>,
  pub root_pid: Option<Pid>,
  pub active_pane: ActivePane,
}

impl App {
  pub fn new(
    tracing_args: &TracingArgs,
    modifier_args: &ModifierArgs,
    pty_master: Option<UnixMasterPty>,
    active_pane: ActivePane,
  ) -> color_eyre::Result<Self> {
    let active_pane = if pty_master.is_some() {
      active_pane
    } else {
      ActivePane::Events
    };
    Ok(Self {
      event_list: EventList::new(),
      printer_args: PrinterArgs::from_cli(tracing_args, modifier_args),
      term: if let Some(pty_master) = pty_master {
        Some(PseudoTerminalPane::new(
          PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
          },
          pty_master,
        )?)
      } else {
        None
      },
      root_pid: None,
      active_pane,
    })
  }

  pub async fn run(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    loop {
      // Handle events
      if let Some(e) = tui.next().await {
        if e != Event::Render {
          log::trace!("Received event {e:?}");
        }
        match e {
          Event::ShouldQuit => {
            action_tx.send(Action::Quit)?;
          }
          Event::Key(ke) => {
            if ke.code == KeyCode::Char('s')
              && ke
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            {
              action_tx.send(Action::SwitchActivePane)?;
              // action_tx.send(Action::Render)?;
            } else {
              log::trace!("TUI: Active pane: {}", self.active_pane);
              if self.active_pane == ActivePane::Events {
                if ke.code == KeyCode::Char('q') {
                  action_tx.send(Action::Quit)?;
                } else if ke.code == KeyCode::Down {
                  action_tx.send(Action::NextItem)?;
                  // action_tx.send(Action::Render)?;
                } else if ke.code == KeyCode::Up {
                  action_tx.send(Action::PrevItem)?;
                  // action_tx.send(Action::Render)?;
                } else if ke.code == KeyCode::Left {
                  action_tx.send(Action::ScrollLeft)?;
                  // action_tx.send(Action::Render)?;
                } else if ke.code == KeyCode::Right {
                  action_tx.send(Action::ScrollRight)?;
                  // action_tx.send(Action::Render)?;
                }
              } else {
                action_tx.send(Action::HandleTerminalKeyPress(ke))?;
                // action_tx.send(Action::Render)?;
              }
            }
          }
          Event::Tracer(te) => match te {
            TracerEvent::RootChildSpawn(pid) => {
              self.root_pid = Some(pid);
            }
            te => {
              self.event_list.items.push(te);
              // action_tx.send(Action::Render)?;
            }
          },
          Event::Render => {
            action_tx.send(Action::Render)?;
          }
          Event::Resize(size) => {
            action_tx.send(Action::Resize(size))?;
            // action_tx.send(Action::Render)?;
          }
          Event::Init => {
            // Fix the size of the terminal
            action_tx.send(Action::Resize(tui.size()?.into()))?;
            // action_tx.send(Action::Render)?;
          }
          Event::Error => {}
        }
      }

      // Handle actions
      while let Ok(action) = action_rx.try_recv() {
        if action != Action::Render {
          log::debug!("action: {action:?}");
        }
        match action {
          Action::Quit => {
            return Ok(());
          }
          Action::Render => {
            tui.draw(|f| self.render(f.size(), f.buffer_mut()))?;
          }
          Action::NextItem => {
            self.event_list.next();
          }
          Action::PrevItem => {
            self.event_list.previous();
          }
          Action::HandleTerminalKeyPress(ke) => {
            if let Some(term) = self.term.as_mut() {
              term.handle_key_event(&ke).await;
            }
          }
          Action::Resize(size) => {
            // Set the window size of the event list
            self.event_list.window = (0, size.height as usize - 4 - 2);
            log::debug!("TUI: set event list window: {:?}", self.event_list.window);

            let term_size = PtySize {
              rows: size.height - 2 - 4,
              cols: size.width / 2 - 2,
              pixel_height: 0,
              pixel_width: 0,
            };

            if let Some(term) = self.term.as_mut() {
              term.resize(term_size)?;
            }
          }
          Action::ScrollLeft => {
            self.event_list.scroll_left();
          }
          Action::ScrollRight => {
            self.event_list.scroll_right();
          }
          Action::SwitchActivePane => {
            self.active_pane = match self.active_pane {
              ActivePane::Events => ActivePane::Terminal,
              ActivePane::Terminal => ActivePane::Events,
            }
          }
        }
      }
    }
  }

  pub fn exit(&self, terminate_on_exit: bool, kill_on_exit: bool) -> color_eyre::Result<()> {
    // Close pty master
    self.term.as_ref().inspect(|t| t.exit());
    // Terminate root process
    if terminate_on_exit {
      self.signal_root_process(Signal::SIGTERM)?;
    } else if kill_on_exit {
      self.signal_root_process(Signal::SIGKILL)?;
    }
    Ok(())
  }

  pub fn signal_root_process(&self, sig: Signal) -> color_eyre::Result<()> {
    if let Some(root_pid) = self.root_pid {
      nix::sys::signal::kill(root_pid, sig)?;
    }
    Ok(())
  }

  fn render_events(&mut self, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
      .title("Events")
      .borders(ratatui::widgets::Borders::ALL)
      .border_style(Style::new().fg(if self.active_pane == ActivePane::Events {
        Color::Cyan
      } else {
        Color::White
      }));
    let mut max_len = area.width as usize - 2;
    // Iterate through all elements in the `items` and stylize them.
    let items = EventList::window(&self.event_list.items, self.event_list.window);
    self.event_list.nr_items_in_window = items.len();
    let items: Vec<ListItem> = items
      .iter()
      .map(|evt| {
        let full_line = evt.to_tui_line(&self.printer_args);
        max_len = max_len.max(full_line.width());
        full_line
          .substring(self.event_list.horizontal_offset, area.width - 2)
          .into()
      })
      .collect();
    // FIXME: It's a little late to set the max width here. The max width is already used
    //        Though this should only affect the first render.
    self.event_list.max_width = max_len;
    // Create a List from all list items and highlight the currently selected one
    let items = List::new(items)
      .highlight_style(
        Style::default()
          .add_modifier(Modifier::BOLD)
          .add_modifier(Modifier::REVERSED)
          .fg(ratatui::style::Color::Cyan),
      )
      .highlight_symbol(">")
      .highlight_spacing(HighlightSpacing::Always)
      .block(block);

    // We can now render the item list
    // (look careful we are using StatefulWidget's render.)
    // ratatui::widgets::StatefulWidget::render as stateful_render
    StatefulWidget::render(items, area, buf, &mut self.event_list.state);
  }
}

impl Widget for &mut App {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // Create a space for header, todo list and the footer.
    let vertical = Layout::vertical([
      Constraint::Length(2),
      Constraint::Min(0),
      Constraint::Length(2),
    ]);
    let [header_area, rest_area, footer_area] = vertical.areas(area);
    let horizontal_constraints = match self.term {
      Some(_) => [Constraint::Percentage(50), Constraint::Percentage(50)],
      None => [Constraint::Percentage(100), Constraint::Length(0)],
    };
    let [left_area, right_area] = Layout::horizontal(horizontal_constraints).areas(rest_area);
    render_title(header_area, buf, "tracexec event list");
    self.render_events(left_area, buf);
    if let Some(term) = self.term.as_mut() {
      term.render(right_area, buf, &mut self.active_pane);
    }
    render_footer(footer_area, buf, "Press 'q' to quit");
  }
}
