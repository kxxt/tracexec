use crossterm::event::KeyEvent;
use nix::sys::signal::Signal;
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
    NewChild,
    Exec,
    RootChildExit {
        signal: Option<Signal>,
        exit_code: i32,
    },
}
