use crossterm::event::KeyEvent;

#[derive(Debug, Clone)]
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
    NewChild,
    Exec,
}
