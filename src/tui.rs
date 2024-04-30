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

use std::{
  ops::{Deref, DerefMut},
  time::Duration,
};

use color_eyre::eyre::Result;
use crossterm::{
  cursor,
  event::{Event as CrosstermEvent, KeyEventKind},
  terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::{backend::CrosstermBackend as Backend, layout::Size};
use tokio::{
  sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::event::{Event, TracerEvent};

pub mod app;
pub mod copy_popup;
pub mod details_popup;
pub mod event_list;
pub mod help;
pub mod partial_line;
pub mod pseudo_term;
pub mod sized_paragraph;
pub mod ui;

pub struct Tui {
  pub terminal: ratatui::Terminal<Backend<std::io::Stderr>>,
  pub task: JoinHandle<()>,
  pub cancellation_token: CancellationToken,
  pub event_rx: UnboundedReceiver<Event>,
  pub event_tx: UnboundedSender<Event>,
  pub frame_rate: f64,
}

pub fn init_tui() -> Result<()> {
  crossterm::terminal::enable_raw_mode()?;
  crossterm::execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide)?;
  Ok(())
}

pub fn restore_tui() -> Result<()> {
  crossterm::execute!(std::io::stdout(), LeaveAlternateScreen, cursor::Show)?;
  crossterm::terminal::disable_raw_mode()?;
  Ok(())
}

impl Tui {
  pub fn new() -> Result<Self> {
    let frame_rate = 30.0;
    let terminal = ratatui::Terminal::new(Backend::new(std::io::stderr()))?;
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let cancellation_token = CancellationToken::new();
    let task = tokio::spawn(async {});
    Ok(Self {
      terminal,
      task,
      cancellation_token,
      event_rx,
      event_tx,
      frame_rate,
    })
  }

  pub fn frame_rate(mut self, frame_rate: f64) -> Self {
    self.frame_rate = frame_rate;
    self
  }

  pub fn start(&mut self, mut tracer_rx: UnboundedReceiver<TracerEvent>) {
    let render_delay = std::time::Duration::from_secs_f64(1.0 / self.frame_rate);
    self.cancel();
    self.cancellation_token = CancellationToken::new();
    let _cancellation_token = self.cancellation_token.clone();
    let _event_tx = self.event_tx.clone();
    self.task = tokio::spawn(async move {
      let mut reader = crossterm::event::EventStream::new();
      let mut render_interval = tokio::time::interval(render_delay);
      _event_tx.send(Event::Init).unwrap();
      loop {
        let render_delay = render_interval.tick();
        let crossterm_event = reader.next().fuse();
        let tracer_event = tracer_rx.recv();
        tokio::select! {
            _ = _cancellation_token.cancelled() => {
                break;
            }
            tracer_event = tracer_event => {
                if let Some(tracer_event) = tracer_event {
                    _event_tx.send(Event::Tracer(tracer_event)).unwrap()
                }
            }
            maybe_event = crossterm_event => {
                match maybe_event {
                    Some(Ok(evt)) => {
                        match evt {
                            CrosstermEvent::Key(key) => {
                                if key.kind == KeyEventKind::Press {
                                    _event_tx.send(Event::Key(key)).unwrap();
                                }
                            },
                            CrosstermEvent::Resize(cols, rows) => {
                                _event_tx.send(Event::Resize(Size {
                                    width: cols,
                                    height: rows,
                                })).unwrap();
                            },
                            _ => {},
                        }
                    }
                    Some(Err(_)) => {
                        _event_tx.send(Event::Error).unwrap();
                    }
                    None => {},
                }
            },
            _ = render_delay => {
                _event_tx.send(Event::Render).unwrap();
            },
        }
      }
    });
  }

  pub fn stop(&self) -> Result<()> {
    self.cancel();
    let mut counter = 0;
    while !self.task.is_finished() {
      std::thread::sleep(Duration::from_millis(1));
      counter += 1;
      if counter > 50 {
        self.task.abort();
      }
      if counter > 100 {
        log::error!("Failed to abort task in 100 milliseconds for unknown reason");
        break;
      }
    }
    Ok(())
  }

  pub fn enter(&mut self, tracer_rx: UnboundedReceiver<TracerEvent>) -> Result<()> {
    init_tui()?;
    self.start(tracer_rx);
    Ok(())
  }

  pub fn exit(&mut self) -> Result<()> {
    self.stop()?;
    if crossterm::terminal::is_raw_mode_enabled()? {
      self.flush()?;
      restore_tui()?;
    }
    Ok(())
  }

  pub fn cancel(&self) {
    self.cancellation_token.cancel();
  }

  pub async fn next(&mut self) -> Option<Event> {
    self.event_rx.recv().await
  }
}

impl Deref for Tui {
  type Target = ratatui::Terminal<Backend<std::io::Stderr>>;

  fn deref(&self) -> &Self::Target {
    &self.terminal
  }
}

impl DerefMut for Tui {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.terminal
  }
}

impl Drop for Tui {
  fn drop(&mut self) {
    self.exit().unwrap();
  }
}
