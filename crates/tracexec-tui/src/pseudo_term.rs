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
  MouseButton,
  MouseEvent,
  MouseEventKind,
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
use tracexec_core::{
  cli::keys::TuiKeyBindings,
  pty::{
    MasterPty,
    PtySize,
    UnixMasterPty,
  },
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

fn encode_key_event(key: &KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
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
    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
      if key.modifiers == KeyModifiers::NONE =>
    {
      let final_byte = match key.code {
        KeyCode::Up => b'A',
        KeyCode::Down => b'B',
        KeyCode::Right => b'C',
        KeyCode::Left => b'D',
        _ => unreachable!(),
      };
      let prefix = if application_cursor { b'O' } else { b'[' };
      vec![ESCAPE, prefix, final_byte]
    }
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
      return None;
    }
    _ => return None,
  };
  Some(input_bytes)
}

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

  pub async fn handle_key_event(&self, key: &KeyEvent, keys: &TuiKeyBindings) -> bool {
    if keys.terminal_toggle_scrollback.matches(*key) {
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

      if keys.terminal_scroll_up.matches(*key) {
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
      if keys.terminal_scroll_down.matches(*key) {
        let current = screen.scrollback();
        if current > 0 {
          screen.set_scrollback(current - 1);
        }
        return true;
      }
      if keys.terminal_page_up.matches(*key) {
        let current = screen.scrollback();
        let available_above = max_offset.saturating_sub(current);
        let step = viewport_height.min(available_above);
        screen.set_scrollback(current + step);
        return true;
      }
      if keys.terminal_page_down.matches(*key) {
        let current = screen.scrollback();
        let step = viewport_height.min(current);
        screen.set_scrollback(current - step);
        return true;
      }
      if keys.terminal_scroll_top.matches(*key) {
        screen.set_scrollback(max_offset);
        return true;
      }
      if keys.terminal_scroll_bottom.matches(*key) {
        screen.set_scrollback(0);
        return true;
      }
      return true;
    }

    let application_cursor = self.parser.read().unwrap().screen().application_cursor();
    let Some(input_bytes) = encode_key_event(key, application_cursor) else {
      return true;
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

  /// Handle a mouse event by converting it to xterm mouse escape sequences
  /// and sending them to the PTY. The `col` and `row` are relative to the
  /// Scroll up by one line in scrollback mode.
  /// Returns true if scrollback mode is active and the scroll was handled.
  pub fn scroll_up(&self) -> bool {
    if !self.scrollback_mode.get() {
      return false;
    }
    let mut parser = self.parser.write().unwrap();
    let screen = parser.screen_mut();
    let current = screen.scrollback();
    if current < self.scrollback_lines {
      screen.set_scrollback(current + 1);
    }
    true
  }

  /// Scroll down by one line in scrollback mode.
  /// Returns true if scrollback mode is active and the scroll was handled.
  pub fn scroll_down(&self) -> bool {
    if !self.scrollback_mode.get() {
      return false;
    }
    let mut parser = self.parser.write().unwrap();
    let screen = parser.screen_mut();
    let current = screen.scrollback();
    if current > 0 {
      screen.set_scrollback(current - 1);
    }
    true
  }

  /// terminal pane's inner area (0-based).
  /// Only sends escape sequences when the terminal has enabled mouse capture,
  /// respecting both the protocol mode and encoding.
  pub async fn handle_mouse_event(&self, event: &MouseEvent, col: u16, row: u16) {
    let (mode, encoding) = {
      let parser = self.parser.read().unwrap();
      let screen = parser.screen();
      (
        screen.mouse_protocol_mode(),
        screen.mouse_protocol_encoding(),
      )
    };

    // Only forward mouse events if the program running in the terminal
    // has enabled mouse reporting (e.g. via DECSET 1000/1002/1003).
    if mode == vt100::MouseProtocolMode::None {
      return;
    }

    // Filter events based on the protocol mode
    let dominated = match mode {
      vt100::MouseProtocolMode::None => unreachable!(),
      // X10: only button press
      vt100::MouseProtocolMode::Press => matches!(
        event.kind,
        MouseEventKind::Down(_)
          | MouseEventKind::ScrollUp
          | MouseEventKind::ScrollDown
          | MouseEventKind::ScrollLeft
          | MouseEventKind::ScrollRight
      ),
      // VT200: button press and release
      vt100::MouseProtocolMode::PressRelease => matches!(
        event.kind,
        MouseEventKind::Down(_)
          | MouseEventKind::Up(_)
          | MouseEventKind::ScrollUp
          | MouseEventKind::ScrollDown
          | MouseEventKind::ScrollLeft
          | MouseEventKind::ScrollRight
      ),
      // Button press, release, and drag (motion with button held)
      vt100::MouseProtocolMode::ButtonMotion => matches!(
        event.kind,
        MouseEventKind::Down(_)
          | MouseEventKind::Up(_)
          | MouseEventKind::Drag(_)
          | MouseEventKind::ScrollUp
          | MouseEventKind::ScrollDown
          | MouseEventKind::ScrollLeft
          | MouseEventKind::ScrollRight
      ),
      // Everything including plain motion
      vt100::MouseProtocolMode::AnyMotion => true,
    };
    if !dominated {
      return;
    }

    let seq = match encoding {
      vt100::MouseProtocolEncoding::Sgr => encode_sgr_mouse(event, col, row),
      // Default and UTF-8 both use the traditional encoding
      vt100::MouseProtocolEncoding::Default | vt100::MouseProtocolEncoding::Utf8 => {
        encode_default_mouse(event, col, row)
      }
    };
    if let Some(seq) = seq {
      self
        .master_tx
        .send(Bytes::from(seq.into_bytes()))
        .await
        .ok();
    }
  }
}

/// Encode a mouse event as an SGR (1006) escape sequence.
fn encode_sgr_mouse(event: &MouseEvent, col: u16, row: u16) -> Option<String> {
  // SGR (1006) mouse encoding: ESC [ < button ; col ; row M/m
  // button: 0 = left, 1 = middle, 2 = right, 64 = scroll up, 65 = scroll down
  // +32 for motion events
  // M = press/motion, m = release
  let (button, is_release) = match event.kind {
    MouseEventKind::Down(MouseButton::Left) => (0u8, false),
    MouseEventKind::Down(MouseButton::Middle) => (1, false),
    MouseEventKind::Down(MouseButton::Right) => (2, false),
    MouseEventKind::Up(MouseButton::Left) => (0, true),
    MouseEventKind::Up(MouseButton::Middle) => (1, true),
    MouseEventKind::Up(MouseButton::Right) => (2, true),
    MouseEventKind::Drag(MouseButton::Left) => (32, false),
    MouseEventKind::Drag(MouseButton::Middle) => (33, false),
    MouseEventKind::Drag(MouseButton::Right) => (34, false),
    MouseEventKind::ScrollUp => (64, false),
    MouseEventKind::ScrollDown => (65, false),
    MouseEventKind::ScrollLeft => (66, false),
    MouseEventKind::ScrollRight => (67, false),
    MouseEventKind::Moved => (35, false),
  };

  // Add modifier flags
  let mut button = button;
  if event.modifiers.contains(KeyModifiers::SHIFT) {
    button += 4;
  }
  if event.modifiers.contains(KeyModifiers::ALT) {
    button += 8;
  }
  if event.modifiers.contains(KeyModifiers::CONTROL) {
    button += 16;
  }

  // SGR format: ESC [ < button ; col+1 ; row+1 M/m
  let suffix = if is_release { 'm' } else { 'M' };
  Some(format!(
    "\x1b[<{};{};{}{}",
    button,
    col + 1,
    row + 1,
    suffix
  ))
}

/// Encode a mouse event in the traditional X10/default format.
/// Returns `None` for events that cannot be represented (release events in
/// default encoding use button=3, coordinates > 222 are unrepresentable).
fn encode_default_mouse(event: &MouseEvent, col: u16, row: u16) -> Option<String> {
  // Traditional encoding: ESC [ M Cb Cx Cy
  // Cb = button + 32, Cx = col + 33, Cy = row + 33
  // Coordinates are limited to 222 (255 - 33)
  if col > 222 || row > 222 {
    return None;
  }

  let button: u8 = match event.kind {
    MouseEventKind::Down(MouseButton::Left) => 0,
    MouseEventKind::Down(MouseButton::Middle) => 1,
    MouseEventKind::Down(MouseButton::Right) => 2,
    MouseEventKind::Up(_) => 3, // Release is encoded as button 3
    MouseEventKind::Drag(MouseButton::Left) => 32,
    MouseEventKind::Drag(MouseButton::Middle) => 33,
    MouseEventKind::Drag(MouseButton::Right) => 34,
    MouseEventKind::ScrollUp => 64,
    MouseEventKind::ScrollDown => 65,
    MouseEventKind::ScrollLeft => 66,
    MouseEventKind::ScrollRight => 67,
    MouseEventKind::Moved => 35,
  };

  let mut button = button;
  if event.modifiers.contains(KeyModifiers::SHIFT) {
    button += 4;
  }
  if event.modifiers.contains(KeyModifiers::ALT) {
    button += 8;
  }
  if event.modifiers.contains(KeyModifiers::CONTROL) {
    button += 16;
  }

  let cb = (button + 32) as char;
  let cx = (col as u8 + 33) as char;
  let cy = (row as u8 + 33) as char;
  Some(format!("\x1b[M{cb}{cx}{cy}"))
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
  use std::{
    sync::LazyLock,
    time::Duration,
  };

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
    cli::keys::TuiKeyBindings,
    pty::{
      PtySize,
      PtySystem,
      native_pty_system,
    },
    tracee,
  };
  use tracing::debug;

  use super::*;

  static KEY_BINDINGS: LazyLock<TuiKeyBindings> = LazyLock::new(TuiKeyBindings::default);

  fn keys() -> &'static TuiKeyBindings {
    &KEY_BINDINGS
  }

  #[test]
  fn encode_arrow_keys_respects_application_cursor_mode() {
    assert_eq!(
      encode_key_event(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), false),
      Some(vec![ESCAPE, b'[', b'C'])
    );
    assert_eq!(
      encode_key_event(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), true),
      Some(vec![ESCAPE, b'O', b'C'])
    );
    assert_eq!(
      encode_key_event(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), true),
      Some(vec![ESCAPE, b'O', b'D'])
    );
    assert_eq!(
      encode_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), true),
      Some(vec![ESCAPE, b'O', b'A'])
    );
    assert_eq!(
      encode_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), true),
      Some(vec![ESCAPE, b'O', b'B'])
    );
  }

  #[test]
  fn encode_modified_arrow_keys_uses_csi_sequence() {
    assert_eq!(
      encode_key_event(&KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL), true),
      Some(vec![ESCAPE, b'[', b'C'])
    );
  }

  #[test]
  fn parser_tracks_application_cursor_mode() {
    let mut parser = vt100::Parser::new(24, 80, 0);

    assert!(!parser.screen().application_cursor());
    parser.process(b"\x1b[?1h");
    assert!(parser.screen().application_cursor());
    parser.process(b"\x1b[?1l");
    assert!(!parser.screen().application_cursor());
  }

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
        .handle_key_event(
          &KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
          keys()
        )
        .await
    );
    assert!(
      term
        .handle_key_event(
          &KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
          keys()
        )
        .await
    );
    assert!(
      term
        .handle_key_event(
          &KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT),
          keys()
        )
        .await
    );
    assert!(
      term
        .handle_key_event(
          &KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
          keys()
        )
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), keys())
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE), keys())
        .await
    );
    assert!(
      term
        .handle_key_event(&KeyEvent::new(KeyCode::F(99), KeyModifiers::NONE), keys())
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
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
      .await;
    assert!(term.is_scrollback_mode());

    // Toggle off with Ctrl+U
    term
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
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
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
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
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
      .await;
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 0);

    // Scroll up (increase offset)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), keys())
      .await;
    assert!(result);
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 1);

    // Scroll up more
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), keys())
      .await;
    assert!(result); // Key is consumed
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 2);

    // Scroll down (decrease offset)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), keys())
      .await;
    assert!(result); // Key is consumed
    assert!(term.is_scrollback_mode());
    assert_eq!(term.scrollback(), 1);

    // Switch back resets offset
    let result = term
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
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
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
      .await;

    // Page up should shift by viewport height (24)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), keys())
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 24);

    // Page up again
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), keys())
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 48);

    // Page down should decrease offset
    let result = term
      .handle_key_event(
        &KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        keys(),
      )
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 24);

    // Page down back to live
    let result = term
      .handle_key_event(
        &KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        keys(),
      )
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
      .handle_key_event(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        keys(),
      )
      .await;

    // Scroll up a bit
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), keys())
      .await;
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), keys())
      .await;
    term
      .handle_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), keys())
      .await;
    assert_eq!(term.scrollback(), 3);

    // Home should jump to max offset
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), keys())
      .await;
    assert!(result);
    // max_offset = 100 (scrollback_lines configured)
    assert_eq!(term.scrollback(), 100);

    // End should jump back to 0 (live)
    let result = term
      .handle_key_event(&KeyEvent::new(KeyCode::End, KeyModifiers::NONE), keys())
      .await;
    assert!(result);
    assert_eq!(term.scrollback(), 0);

    Ok(())
  }

  #[test]
  fn handle_mouse_event_sgr_encoding() -> color_eyre::Result<()> {
    use super::encode_sgr_mouse;

    // Left button down at col=5, row=3
    let mouse = MouseEvent {
      kind: MouseEventKind::Down(MouseButton::Left),
      column: 10,
      row: 5,
      modifiers: KeyModifiers::NONE,
    };
    assert_eq!(encode_sgr_mouse(&mouse, 5, 3), Some("\x1b[<0;6;4M".into()));

    // Scroll up at col=2, row=1
    let mouse = MouseEvent {
      kind: MouseEventKind::ScrollUp,
      column: 10,
      row: 5,
      modifiers: KeyModifiers::NONE,
    };
    assert_eq!(encode_sgr_mouse(&mouse, 2, 1), Some("\x1b[<64;3;2M".into()));

    // Release with shift modifier
    let mouse = MouseEvent {
      kind: MouseEventKind::Up(MouseButton::Left),
      column: 10,
      row: 5,
      modifiers: KeyModifiers::SHIFT,
    };
    assert_eq!(encode_sgr_mouse(&mouse, 5, 3), Some("\x1b[<4;6;4m".into()));

    // Right button drag with control
    let mouse = MouseEvent {
      kind: MouseEventKind::Drag(MouseButton::Right),
      column: 0,
      row: 0,
      modifiers: KeyModifiers::CONTROL,
    };
    assert_eq!(
      encode_sgr_mouse(&mouse, 10, 20),
      Some("\x1b[<50;11;21M".into())
    );

    // Moved event returns button=35 (motion without button)
    let mouse = MouseEvent {
      kind: MouseEventKind::Moved,
      column: 0,
      row: 0,
      modifiers: KeyModifiers::NONE,
    };
    assert_eq!(encode_sgr_mouse(&mouse, 0, 0), Some("\x1b[<35;1;1M".into()));

    Ok(())
  }

  #[test]
  fn handle_mouse_event_default_encoding() -> color_eyre::Result<()> {
    use super::encode_default_mouse;

    // Left button down at col=0, row=0
    let mouse = MouseEvent {
      kind: MouseEventKind::Down(MouseButton::Left),
      column: 10,
      row: 5,
      modifiers: KeyModifiers::NONE,
    };
    // button=0 → Cb=32=' ', col=0+33=33='!', row=0+33=33='!'
    assert_eq!(encode_default_mouse(&mouse, 0, 0), Some("\x1b[M !!".into()));

    // Release → button=3 → Cb=35='#'
    let mouse = MouseEvent {
      kind: MouseEventKind::Up(MouseButton::Left),
      column: 10,
      row: 5,
      modifiers: KeyModifiers::NONE,
    };
    assert_eq!(encode_default_mouse(&mouse, 5, 3), Some("\x1b[M#&$".into()));

    // Coordinates > 222 are not representable
    let mouse = MouseEvent {
      kind: MouseEventKind::Down(MouseButton::Left),
      column: 10,
      row: 5,
      modifiers: KeyModifiers::NONE,
    };
    assert_eq!(encode_default_mouse(&mouse, 223, 0), None);
    assert_eq!(encode_default_mouse(&mouse, 0, 223), None);

    Ok(())
  }
}
