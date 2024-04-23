use std::sync::Arc;

use std::sync::RwLock;

use crate::pty::{MasterPty, PtySize, UnixMasterPty};

pub struct PseudoTerminalPane {
    // cannot move out of `parser` because it is borrowed
    // term: PseudoTerminal<'a, Screen>,
    pub parser: Arc<RwLock<vt100::Parser>>,
    pty_master: UnixMasterPty,
    reader_task: tokio::task::JoinHandle<color_eyre::Result<()>>,
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

        Ok(Self {
            // term,
            parser,
            pty_master,
            reader_task,
        })
    }
}
