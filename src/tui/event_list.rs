use crossterm::event::KeyCode;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{HighlightSpacing, List, ListItem, ListState, StatefulWidget, Widget},
};
use tokio::sync::mpsc;

use crate::{
    event::{Action, Event, TracerEvent},
    printer::PrinterArgs,
};

use super::{
    ui::{render_footer, render_title},
    Tui,
};

pub struct EventList {
    state: ListState,
    items: Vec<TracerEvent>,
    last_selected: Option<usize>,
}

impl EventList {
    pub fn new() -> EventList {
        Self {
            state: ListState::default(),
            items: vec![],
            last_selected: None,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => self.last_selected.unwrap_or(0),
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.last_selected.unwrap_or(0),
        };
        self.state.select(Some(i));
    }

    pub fn unselect(&mut self) {
        let offset = self.state.offset();
        self.last_selected = self.state.selected();
        self.state.select(None);
        *self.state.offset_mut() = offset;
    }
}

pub struct EventListApp {
    pub event_list: EventList,
    pub printer_args: PrinterArgs,
}

impl EventListApp {
    pub async fn run(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();

        loop {
            // Handle events
            if let Some(e) = tui.next().await {
                match e {
                    Event::ShouldQuit => {
                        action_tx.send(Action::Quit)?;
                    }
                    Event::Key(ke) => {
                        if ke.code == KeyCode::Char('q') {
                            action_tx.send(Action::Quit)?;
                        } else if ke.code == KeyCode::Down {
                            action_tx.send(Action::NextItem)?;
                            action_tx.send(Action::Render)?;
                        } else if ke.code == KeyCode::Up {
                            action_tx.send(Action::PrevItem)?;
                            action_tx.send(Action::Render)?;
                        }
                    }
                    Event::Tracer(te) => {
                        self.event_list.items.push(te);
                        action_tx.send(Action::Render)?;
                    }
                    Event::Render => {
                        action_tx.send(Action::Render)?;
                    }
                    Event::Init => {}
                    Event::Error => {}
                }
            }

            // Handle actions
            while let Ok(action) = action_rx.try_recv() {
                if action != Action::Render {
                    log::debug!("action: {action:?}");
                }
                match action {
                    Action::Quit => {
                        return Ok(());
                    }
                    Action::Render => {
                        tui.draw(|f| self.render(f.size(), f.buffer_mut()))?;
                    }
                    Action::NextItem => {
                        self.event_list.next();
                    }
                    Action::PrevItem => {
                        self.event_list.previous();
                    }
                }
            }
        }
    }

    fn render_events(&mut self, area: Rect, buf: &mut Buffer) {
        // Iterate through all elements in the `items` and stylize them.
        let items: Vec<ListItem> = self
            .event_list
            .items
            .iter()
            .enumerate()
            .map(|(i, evt)| evt.to_tui_line(&self.printer_args).into())
            .collect();
        // Create a List from all list items and highlight the currently selected one
        let items = List::new(items)
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
                    .fg(ratatui::style::Color::Cyan),
            )
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);

        // We can now render the item list
        // (look careful we are using StatefulWidget's render.)
        // ratatui::widgets::StatefulWidget::render as stateful_render
        StatefulWidget::render(items, area, buf, &mut self.event_list.state);
    }
}

impl Widget for &mut EventListApp {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Create a space for header, todo list and the footer.
        let vertical = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ]);
        let [header_area, rest_area, footer_area] = vertical.areas(area);

        render_title(header_area, buf, "tracexec event list");
        self.render_events(rest_area, buf);
        render_footer(footer_area, buf, "Press 'q' to quit");
    }
}
