// MIT License

// Copyright (c) 2023 a-kenji
// Copyright (c) 2024 Levi Zim

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{
  cell::Cell,
  io::{
    BufWriter,
    Write,
  },
  ops::Deref,
  sync::{
    Arc,
    RwLock,
  },
};

use bytes::Bytes;
use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use ratatui::{
  prelude::{
    Buffer,
    Rect,
  },
  widgets::Widget,
};
use tokio::sync::mpsc::channel;
use tokio_util::sync::CancellationToken;
use tracexec_core::pty::{
  MasterPty,
  PtySize,
  UnixMasterPty,
};
use tracing::{
  trace,
  warn,
};
use tui_term::widget::{
  Cursor,
  PseudoTerminal,
};
use vt100::Parser;

pub struct PseudoTerminalPane {
  // cannot move out of `parser` because it is borrowed
  // term: PseudoTerminal<'a, Screen>,
  pub parser: Arc<RwLock<vt100::Parser>>,
  pty_master: UnixMasterPty,
  #[allow(unused)]
  reader_task: tokio::task::JoinHandle<color_eyre::Result<()>>,
  #[allow(unused)]
  writer_task: tokio::task::JoinHandle<color_eyre::Result<()>>,
  master_tx: tokio::sync::mpsc::Sender<Bytes>,
  master_cancellation_token: CancellationToken,
  size: PtySize,
  focus: bool,
  scrollback_mode: Cell<bool>,
  scrollback_lines: usize,
}

const ESCAPE: u8 = 27;

impl PseudoTerminalPane {
  pub fn new(
    size: PtySize,
    pty_master: UnixMasterPty,
    scrollback_lines: usize,
  ) -> color_eyre::Result<Self> {
    let parser = vt100::Parser::new(size.rows, size.cols, scrollback_lines);
    let parser = Arc::new(RwLock::new(parser));

    let reader_task = {
      let mut reader = pty_master.try_clone_reader()?;
      let parser = parser.clone();
      tokio::task::spawn_blocking(move || {
        let mut processed_buf = Vec::new();
        let mut buf = [0u8; 8192];

        loop {
          let size = reader.read(&mut buf)?;
          if size == 0 {
            break;
          }
          if size > 0 {
            processed_buf.extend_from_slice(&buf[..size]);
            parser.write().unwrap().process(&processed_buf);

            // Clear the processed portion of the buffer
            processed_buf.clear();
          }
        }
        Ok(())
      })
    };

    let (tx, mut rx) = channel::<Bytes>(32);
    let master_cancellation_token = CancellationToken::new();

    let writer_task = {
      let cancellation_token = master_cancellation_token.clone();
      let mut writer = BufWriter::new(pty_master.take_writer()?);
      // Drop writer on purpose
      tokio::spawn(async move {
        loop {
          tokio::select! {
            _ = cancellation_token.cancelled() => break,
            Some(bytes) = rx.recv() => {
              writer.write_all(&bytes)?;
              writer.flush()?;
            }
          }
        }
        trace!("Closing pty master!");
        Ok(())
      })
    };

    Ok(Self {
      // term,
      size,
      parser,
      pty_master,
      reader_task,
      writer_task,
      master_tx: tx,
      master_cancellation_token,
      focus: false,
      scrollback_mode: Cell::new(false),
      scrollback_lines,
    })
  }

  pub async fn handle_key_event(&self, key: &KeyEvent) -> bool {
    if let KeyCode::Char(ch) = key.code
      && (ch == 'u' || ch == 'U')
      && key.modifiers == KeyModifiers::CONTROL
    {
      self.scrollback_mode.set(!self.scrollback_mode.get());
      let mut parser = self.parser.write().unwrap();
      let screen = parser.screen_mut();
      screen.set_scrollback(0);
      return true;
    }

    // Handle scrollback navigation when in scrollback mode
    if self.scrollback_mode.get() {
      let mut parser = self.parser.write().unwrap();
      let screen = parser.screen_mut();
      let viewport_height = self.size.rows as usize;
      let max_offset = self.scrollback_lines;
      // .min(screen.scrollback_len()) Waiting for https://github.com/doy/vt100-rust/pull/27

      match key.code {
        KeyCode::Up => {
          let current = screen.scrollback();
          if current < max_offset {
            trace!(
              "Scrolling up: current={}, max_offset={}",
              current, max_offset
            );
            screen.set_scrollback(current + 1);
            trace!("New scrollback offset: {}", screen.scrollback());
          }
          return true;
        }
        KeyCode::Down => {
          let current = screen.scrollback();
          if current > 0 {
            screen.set_scrollback(current - 1);
          }
          return true;
        }
        KeyCode::PageUp => {
          let current = screen.scrollback();
          let available_above = max_offset.saturating_sub(current);
          let step = viewport_height.min(available_above);
          screen.set_scrollback(current + step);
          return true;
        }
        KeyCode::PageDown => {
          let current = screen.scrollback();
          let step = viewport_height.min(current);
          screen.set_scrollback(current - step);
          return true;
        }
        KeyCode::Home => {
          screen.set_scrollback(max_offset);
          return true;
        }
        KeyCode::End => {
          screen.set_scrollback(0);
          return true;
        }
        _ => {}
      }
      return true;
    }

    let input_bytes = match key.code {
      KeyCode::Char(ch) => {
        let mut send = vec![0; 4];
        ch.encode_utf8(&mut send);
        send.drain(ch.len_utf8()..);
        if ch.is_ascii() && key.modifiers == KeyModifiers::CONTROL {
          let char = ch.to_ascii_uppercase();
          // https://github.com/fyne-io/terminal/blob/master/input.go
          // https://gist.github.com/ConnerWill/d4b6c776b509add763e17f9f113fd25b
          match char {
            '2' | '@' | ' ' => send = vec![0],
            '3' | '[' => send = vec![27],
            '4' | '\\' => send = vec![28],
            '5' | ']' => send = vec![29],
            '6' | '^' => send = vec![30],
            '7' | '-' | '_' => send = vec![31],
            char if ('A'..='_').contains(&char) => {
              // Since A == 65,
              // we can safely subtract 64 to get
              // the corresponding control character
              let ascii_val = char as u8;
              let ascii_to_send = ascii_val - 64;
              send = vec![ascii_to_send];
            }
            _ => {}
          }
        } else if key.modifiers == KeyModifiers::ALT {
          send = vec![ESCAPE, ch as u8];
        }
        send
      }
      KeyCode::Enter => vec![b'\n'],
      KeyCode::Backspace => vec![8],
      KeyCode::Left => vec![ESCAPE, b'[', b'D'],
      KeyCode::Right => vec![ESCAPE, b'[', b'C'],
      KeyCode::Up => vec![ESCAPE, b'[', b'A'],
      KeyCode::Down => vec![ESCAPE, b'[', b'B'],
      KeyCode::Tab => vec![b'\t'],
      KeyCode::Home => vec![ESCAPE, b'O', b'H'],
      KeyCode::End => vec![ESCAPE, b'O', b'F'],
      KeyCode::PageUp => vec![ESCAPE, b'[', b'5', b'~'],
      KeyCode::PageDown => vec![ESCAPE, b'[', b'6', b'~'],
      KeyCode::BackTab => vec![ESCAPE, b'[', b'Z'],
      KeyCode::Delete => vec![ESCAPE, b'[', b'3', b'~'],
      KeyCode::Insert => vec![ESCAPE, b'[', b'2', b'~'],
      KeyCode::Esc => vec![ESCAPE],
      KeyCode::F(1) => vec![ESCAPE, b'O', b'P'],
      KeyCode::F(2) => vec![ESCAPE, b'O', b'Q'],
      KeyCode::F(3) => vec![ESCAPE, b'O', b'R'],
      KeyCode::F(4) => vec![ESCAPE, b'O', b'S'],
      KeyCode::F(5) => vec![ESCAPE, b'[', b'1', b'5', b'~'],
      KeyCode::F(6) => vec![ESCAPE, b'[', b'1', b'7', b'~'],
      KeyCode::F(7) => vec![ESCAPE, b'[', b'1', b'8', b'~'],
      KeyCode::F(8) => vec![ESCAPE, b'[', b'1', b'9', b'~'],
      KeyCode::F(9) => vec![ESCAPE, b'[', b'2', b'0', b'~'],
      KeyCode::F(10) => vec![ESCAPE, b'[', b'2', b'1', b'~'],
      KeyCode::F(11) => vec![ESCAPE, b'[', b'2', b'3', b'~'],
      KeyCode::F(12) => vec![ESCAPE, b'[', b'2', b'4', b'~'],
      KeyCode::F(n) => {
        // TODO: Handle Other F keys
        warn!("Unhandled F key: {}", n);
        return true;
      }
      _ => return true,
    };

    self.master_tx.send(Bytes::from(input_bytes)).await.ok();
    true
  }

  pub fn resize(&mut self, size: PtySize) -> color_eyre::Result<()> {
    if size == self.size {
      return Ok(());
    }
    self.size = size;
    self
      .parser
      .write()
      .unwrap()
      .screen_mut()
      .set_size(size.rows, size.cols);
    self.pty_master.resize(size)?;
    Ok(())
  }

  pub fn focus(&mut self, focus: bool) {
    self.focus = focus;
  }

  pub fn is_scrollback_mode(&self) -> bool {
    self.scrollback_mode.get()
  }

  pub fn scrollback(&self) -> usize {
    self.parser.read().unwrap().screen().scrollback()
  }

  pub fn parser(&self) -> impl Deref<Target = Parser> {
    self.parser.read().unwrap()
  }

  /// Closes pty master
  pub fn exit(&self) {
    self.master_cancellation_token.cancel()
  }
}

impl Widget for &PseudoTerminalPane {
  fn render(self, area: Rect, buf: &mut Buffer)
  where
    Self: Sized,
  {
    let parser = self.parser.read().unwrap();
    let mut cursor = Cursor::default();
    if !self.focus {
      cursor.hide();
    }

    let screen = parser.screen();

    let pseudo_term = PseudoTerminal::new(screen).cursor(cursor);
    pseudo_term.render(area, buf);
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
  };
  use ratatui::prelude::{
    Buffer,
    Rect,
  };
  use tracexec_core::{
    pty::{
      PtySize,
      PtySystem,
      native_pty_system,
    },
    tracee,
  };
  use tracing::debug;

  use super::*;

  #[tokio::test]
  async fn pseudo_terminal_handle_key_event_various_keys() -> color_eyre::Result<()> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system.openpty(PtySize {
      rows: 12,
      cols: 40,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    let mut term = PseudoTerminalPane::new(
      PtySize {
        rows: 12,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
      },
      pty_pair.master,
      10,
    )?;

    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT))
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE))
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::F(99), KeyModifiers::NONE))
        .await
    );

    // Verify resize and focus operations cover the respective paths.
    term.resize(PtySize {
      rows: 20,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    assert_eq!(term.size.rows, 20);
    assert_eq!(term.size.cols, 80);

    term.focus(true);
    assert!(term.focus);
    term.focus(false);
    assert!(!term.focus);

    // Rendering should not panic and uses currently captured parser state.
    let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 5));
    (&term).render(Rect::new(0, 0, 20, 5), &mut buffer);

    term.exit();

    // It should not hang after calling exit

    Ok(())
  }

  #[tokio::test]
  async fn scrollback_toggle_mode() -> color_eyre::Result<()> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system.openpty(PtySize {
      rows: 12,
      cols: 40,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    let term = PseudoTerminalPane::new(
      PtySize {
        rows: 12,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
      },
      pty_pair.master,
      10,
    )?;

    // Initially not in scrollback mode
    assert!(!term.is_scrollback_mode());

    // Toggle on with Ctrl+U
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;
    assert!(term.is_scrollback_mode());

    // Toggle off with Ctrl+U
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;
    assert!(!term.is_scrollback_mode());

    Ok(())
  }

  #[tokio::test]
  async fn scrollback_get_initial_state() -> color_eyre::Result<()> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system.openpty(PtySize {
      rows: 12,
      cols: 40,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    let term = PseudoTerminalPane::new(
      PtySize {
        rows: 12,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
      },
      pty_pair.master,
      100,
    )?;

    // Initial scrollback offset should be 0 (live view)
    assert_eq!(term.scrollback(), 0);

    // After entering scrollback mode, it should still be 0 (reset to live)
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 0);

    Ok(())
  }

  #[tokio::test]
  #[tracing_test::traced_test]
  async fn scrollback_scroll_up_down() -> color_eyre::Result<()> {
    // console_subscriber::init();
    use nix::sys::wait::waitpid;
    use tracexec_core::{
      cmdbuilder::CommandBuilder,
      pty,
    };

    let pty_system = native_pty_system();
    let pty_pair = pty_system.openpty(PtySize {
      rows: 3,
      cols: 40,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    let term = PseudoTerminalPane::new(
      PtySize {
        rows: 3,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
      },
      pty_pair.master,
      100,
    )?;

    // Spawn a shell command through the PTY that generates 150 lines of output
    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg("for i in $(seq 1 150); do echo \"Line $i: test output\"; done");

    debug!("Spawning command through PTY: {:?}", cmd);

    let child_pid = pty::spawn_command(Some(&pty_pair.slave), cmd, move |_| {
      tracee::lead_session_and_control_terminal()?;
      Ok(())
    })?;

    // Reap the child process
    waitpid(child_pid, None)?;

    // Wait for the reader task to process all output and update the parser state
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Enter scrollback mode
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 0);

    // Scroll up (increase offset)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
      .await;
    assert!(result);
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 1);

    // Scroll up more
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
      .await;
    assert!(result); // Key is consumed
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 2);

    // Scroll down (decrease offset)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
      .await;
    assert!(result); // Key is consumed
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 1);

    // Switch back resets offset
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;
    assert!(result); // Key is consumed
    assert!(!term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 0);

    Ok(())
  }

  #[tokio::test]
  async fn scrollback_page_navigation() -> color_eyre::Result<()> {
    use nix::sys::wait::waitpid;
    use tracexec_core::{
      cmdbuilder::CommandBuilder,
      pty,
    };

    let pty_system = native_pty_system();
    let pty_pair = pty_system.openpty(PtySize {
      rows: 24,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    let term = PseudoTerminalPane::new(
      PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
      },
      pty_pair.master,
      100,
    )?;

    // Spawn a shell command through the PTY that generates 100 lines of output
    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg("for i in $(seq 1 100); do echo \"Line $i\"; done");

    let child_pid = pty::spawn_command(Some(&pty_pair.slave), cmd, move |_| {
      tracee::lead_session_and_control_terminal()?;
      Ok(())
    })?;

    // Reap child
    waitpid(child_pid, None)?;

    // Give the reader task time to process the output
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Enter scrollback mode
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;

    // Page up should shift by viewport height (24)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 24);

    // Page up again
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 48);

    // Page down should decrease offset
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 24);

    // Page down back to live
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 0);

    Ok(())
  }

  #[tokio::test]
  async fn scrollback_home_end_navigation() -> color_eyre::Result<()> {
    use nix::sys::wait::waitpid;
    use tracexec_core::{
      cmdbuilder::CommandBuilder,
      pty,
    };

    let pty_system = native_pty_system();
    let pty_pair = pty_system.openpty(PtySize {
      rows: 12,
      cols: 40,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    let term = PseudoTerminalPane::new(
      PtySize {
        rows: 12,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
      },
      pty_pair.master,
      100,
    )?;

    // Spawn a shell command through the PTY that generates 120 lines of output
    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg("for i in $(seq 1 120); do echo \"Line $i\"; done");

    let child_pid = pty::spawn_command(Some(&pty_pair.slave), cmd, move |_| {
      tracee::lead_session_and_control_terminal()?;
      Ok(())
    })?;
    // Give the reader task time to process the output
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Reap child
    waitpid(child_pid, None)?;

    // Enter scrollback mode
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
      .await;

    // Scroll up a bit
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
      .await;
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
      .await;
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
      .await;
    assert_eq!(term.scrollback(), 3);

    // Home should jump to max offset
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
      .await;
    assert!(result);
    // max_offset = 100 (scrollback_lines configured)
    assert_eq!(term.scrollback(), 100);

    // End should jump back to 0 (live)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::End, KeyModifiers::NONE))
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 0);

    Ok(())
  }
}
