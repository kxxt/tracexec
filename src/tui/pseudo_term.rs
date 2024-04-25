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

use bytes::Bytes;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::{BufWriter, Write};
use std::sync::Arc;
use tokio::sync::mpsc::channel;

use tokio_util::sync::CancellationToken;

use std::sync::RwLock;

use crate::pty::{MasterPty, PtySize, UnixMasterPty};

pub struct PseudoTerminalPane {
  // cannot move out of `parser` because it is borrowed
  // term: PseudoTerminal<'a, Screen>,
  pub parser: Arc<RwLock<vt100::Parser>>,
  pty_master: UnixMasterPty,
  reader_task: tokio::task::JoinHandle<color_eyre::Result<()>>,
  writer_task: tokio::task::JoinHandle<color_eyre::Result<()>>,
  master_tx: tokio::sync::mpsc::Sender<Bytes>,
  master_cancellation_token: CancellationToken,
  size: PtySize,
}

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
            let mut parser = parser.write().unwrap();
            parser.process(&processed_buf);

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
        log::trace!("Closing pty master!");
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
    })
  }

  pub async fn handle_key_event(&self, key: &KeyEvent) -> bool {
    let input_bytes = match key.code {
      KeyCode::Char(ch) => {
        let mut send = vec![ch as u8];
        if key.modifiers == KeyModifiers::CONTROL {
          match ch {
            'n' => {
              // Ignore Ctrl+n within a pane
              return true;
            }
            'x' => {
              // Close the pane
              return false;
            }
            _ => {
              let char = ch.to_ascii_uppercase();
              let ascii_val = char as u8;
              // Since char is guaranteed to be an ASCII character,
              // we can safely subtract 64 to get
              // the corresponding control character
              let ascii_to_send = ascii_val - 64;
              send = vec![ascii_to_send];
            }
          }
        }
        send
      }
      KeyCode::Enter => vec![b'\n'],
      KeyCode::Backspace => vec![8],
      KeyCode::Left => vec![27, 91, 68],
      KeyCode::Right => vec![27, 91, 67],
      KeyCode::Up => vec![27, 91, 65],
      KeyCode::Down => vec![27, 91, 66],
      KeyCode::Tab => vec![9],
      KeyCode::Home => vec![27, 91, 72],
      KeyCode::End => vec![27, 91, 70],
      KeyCode::PageUp => vec![27, 91, 53, 126],
      KeyCode::PageDown => vec![27, 91, 54, 126],
      KeyCode::BackTab => vec![27, 91, 90],
      KeyCode::Delete => vec![27, 91, 51, 126],
      KeyCode::Insert => vec![27, 91, 50, 126],
      KeyCode::Esc => vec![27],
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
    let mut parser = self.parser.write().unwrap();
    parser.set_size(size.rows, size.cols);
    self.pty_master.resize(size)?;
    Ok(())
  }

  /// Closes pty master
  pub fn exit(&self) {
    self.master_cancellation_token.cancel()
  }
}
