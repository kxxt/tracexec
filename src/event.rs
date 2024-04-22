use crossterm::event::KeyEvent;
use nix::{sys::signal::Signal, unistd::Pid};
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

#[derive(Debug, Clone, Display)]
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
