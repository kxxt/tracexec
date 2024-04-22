use crossterm::event::KeyEvent;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
    style::{Color, Stylize},
    text::Line,
};
use strum::Display;

#[derive(Debug, Clone, Display)]
pub enum Event {
    ShouldQuit,
    Key(KeyEvent),
    Tracer(TracerEvent),
    Render,
    Init,
    Error,
}

#[derive(Debug, Clone)]
pub enum TracerEvent {
    Info,
    Warning,
    Error,
    FatalError,
    NewChild {
        ppid: Pid,
        pcomm: String,
        pid: Pid,
    },
    Exec,
    RootChildExit {
        signal: Option<Signal>,
        exit_code: i32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Render,
    NextItem,
    PrevItem,
}

impl TracerEvent {
    pub fn to_tui_line(&self) -> Line {
        match self {
            TracerEvent::Info => "Info".into(),
            TracerEvent::Warning => "Warning".into(),
            TracerEvent::Error => "Error".into(),
            TracerEvent::FatalError => "FatalError".into(),
            TracerEvent::NewChild { ppid, pcomm, pid } => Line::from(vec![
                ppid.to_string().fg(Color::Yellow),
                format!("<{}>", pcomm).fg(Color::Cyan),
                ": ".into(),
                "new child".fg(Color::Magenta),
                ": ".into(),
                pid.to_string().fg(Color::Yellow),
            ]),
            TracerEvent::Exec => "Exec".into(),
            TracerEvent::RootChildExit { signal, exit_code } => format!(
                "RootChildExit: signal: {:?}, exit_code: {}",
                signal, exit_code
            )
            .into(),
        }
    }
}
