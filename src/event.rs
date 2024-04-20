use crossterm::event::KeyEvent;

#[derive(Debug, Clone)]
pub enum Event {
    ShouldQuit,
    NewChild,
    Exec,
    Error,
    Warning,
    Info,
    Key(KeyEvent),
    Render,
    Init,
}
