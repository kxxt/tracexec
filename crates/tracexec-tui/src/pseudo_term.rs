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
  io::{
    BufWriter,
    Write,
  },
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
}

const ESCAPE: u8 = 27;

impl PseudoTerminalPane {
  pub fn new(size: PtySize, pty_master: UnixMasterPty) -> color_eyre::Result<Self> {
    let parser = vt100::Parser::new(size.rows, size.cols, 0);
    // let screen = parser.screen();
    let parser = Arc::new(RwLock::new(parser));
    // let term = PseudoTerminal::new(screen);

    let reader_task = {
      let mut reader = pty_master.try_clone_reader()?;
      let parser = parser.clone();
      tokio::spawn(async move {
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
    })
  }

  pub async fn handle_key_event(&self, key: &KeyEvent) -> bool {
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
    self.parser.write().unwrap().set_size(size.rows, size.cols);
    self.pty_master.resize(size)?;
    Ok(())
  }

  pub fn focus(&mut self, focus: bool) {
    self.focus = focus;
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
    let pseudo_term = PseudoTerminal::new(parser.screen()).cursor(cursor);
    pseudo_term.render(area, buf);
  }
}
