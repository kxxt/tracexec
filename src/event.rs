use std::path::PathBuf;

use crossterm::event::KeyEvent;
use itertools::chain;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
    layout::Size,
    style::{Color, Stylize},
    text::Line,
};
use strum::Display;

use crate::{printer::PrinterArgs, proc::Interpreter};

#[derive(Debug, Clone, Display)]
pub enum Event {
    ShouldQuit,
    Key(KeyEvent),
    Tracer(TracerEvent),
    Render,
    Resize(Size),
    Init,
    Error,
}

#[derive(Debug, Clone)]
pub enum TracerEvent {
    Info(TracerMessage),
    Warning(TracerMessage),
    Error(TracerMessage),
    FatalError,
    NewChild {
        ppid: Pid,
        pcomm: String,
        pid: Pid,
    },
    Exec(ExecEvent),
    RootChildSpawn(Pid),
    RootChildExit {
        signal: Option<Signal>,
        exit_code: i32,
    },
}

#[derive(Debug, Clone)]
pub struct TracerMessage {
    pub pid: Option<Pid>,
    pub msg: String,
}

#[derive(Debug, Clone)]
pub struct ExecEvent {
    pub pid: Pid,
    pub cwd: PathBuf,
    pub comm: String,
    pub filename: PathBuf,
    pub argv: Vec<String>,
    pub interpreter: Vec<Interpreter>,
    pub envp: Vec<String>,
    pub result: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Render,
    Resize(Size),
    NextItem,
    PrevItem,
    ScrollLeft,
    ScrollRight,
    SwitchActivePane,
    HandleTerminalKeyPress(KeyEvent),
}

macro_rules! tracer_event_spans {
    ($pid: expr, $comm: expr, $printer_args: expr, $($t:tt)*) => {
        chain!([
            Some($pid.to_string().fg(Color::Yellow)),
            $printer_args
                .trace_comm
                .then_some(format!("<{}>", $comm).fg(Color::Cyan)),
            Some(": ".into()),
        ], [$($t)*])
    };
}

impl TracerEvent {
    pub fn to_tui_line(&self, args: &PrinterArgs) -> Line {
        match self {
            TracerEvent::Info(TracerMessage { ref msg, pid }) => chain!(
                ["info".bg(Color::LightBlue)],
                pid.map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
                    .unwrap_or_default(),
                [": ".into(), msg.as_str().into()]
            )
            .collect(),
            TracerEvent::Warning(TracerMessage { ref msg, pid }) => chain!(
                ["warn".bg(Color::Yellow)],
                pid.map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
                    .unwrap_or_default(),
                [": ".into(), msg.as_str().into()]
            )
            .collect(),
            TracerEvent::Error(TracerMessage { ref msg, pid }) => chain!(
                ["error".bg(Color::Red)],
                pid.map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
                    .unwrap_or_default(),
                [": ".into(), msg.as_str().into()]
            )
            .collect(),
            TracerEvent::FatalError => "FatalError".into(),
            TracerEvent::NewChild { ppid, pcomm, pid } => {
                let spans = tracer_event_spans!(
                    ppid,
                    pcomm,
                    args,
                    Some("new child ".fg(Color::Magenta)),
                    Some(pid.to_string().fg(Color::Yellow)),
                );
                spans.flatten().collect()
            }
            TracerEvent::Exec(ExecEvent {
                pid,
                cwd: _,
                comm,
                filename,
                argv,
                interpreter,
                envp,
                result,
            }) => {
                let spans = tracer_event_spans!(
                    pid,
                    comm,
                    args,
                    Some("exec ".fg(Color::Magenta)),
                    Some(filename.display().to_string().fg(Color::Green)),
                    Some(" argv: [".into()),
                    Some(argv.join(", ").fg(Color::Green)),
                    Some("]".into()),
                    args.trace_interpreter.then_some(" interpreter: [".into()),
                    args.trace_interpreter.then_some(
                        interpreter
                            .iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                            .fg(Color::Green)
                    ),
                    args.trace_interpreter.then_some("]".into()),
                    Some(" envp: [".into()),
                    Some(envp.join(", ").fg(Color::Green)),
                    Some("] result: ".into()),
                    Some(result.to_string().fg(Color::Yellow)),
                );
                spans.flatten().collect()
            }
            TracerEvent::RootChildExit { signal, exit_code } => format!(
                "RootChildExit: signal: {:?}, exit_code: {}",
                signal, exit_code
            )
            .into(),
            TracerEvent::RootChildSpawn(pid) => format!("RootChildSpawn: {}", pid).into(),
        }
    }
}
