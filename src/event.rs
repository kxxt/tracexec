use crossterm::event::KeyEvent;
use itertools::chain;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
    style::{Color, Stylize},
    text::Line,
};
use strum::Display;

use crate::printer::PrinterArgs;

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
    Info(TracerMessage),
    Warning(TracerMessage),
    Error(TracerMessage),
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

#[derive(Debug, Clone)]
pub struct TracerMessage {
    pub pid: Option<Pid>,
    pub msg: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Render,
    NextItem,
    PrevItem,
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
            .into_iter()
            .collect(),
            TracerEvent::Warning(TracerMessage { ref msg, pid }) => chain!(
                ["warn".bg(Color::Yellow)],
                pid.map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
                    .unwrap_or_default(),
                [": ".into(), msg.as_str().into()]
            )
            .into_iter()
            .collect(),
            TracerEvent::Error(TracerMessage { ref msg, pid }) => chain!(
                ["error".bg(Color::Red)],
                pid.map(|p| ["(".into(), p.to_string().fg(Color::Yellow), ")".into()])
                    .unwrap_or_default(),
                [": ".into(), msg.as_str().into()]
            )
            .into_iter()
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
                spans.into_iter().flatten().collect()
            }
            TracerEvent::Exec => "Exec".into(),
            TracerEvent::RootChildExit { signal, exit_code } => format!(
                "RootChildExit: signal: {:?}, exit_code: {}",
                signal, exit_code
            )
            .into(),
        }
    }
}
