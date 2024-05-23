use std::{collections::BTreeMap, sync::Arc};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
  layout::Alignment,
  prelude::{Buffer, Rect},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, StatefulWidgetRef, Widget},
};
use tui_widget_list::PreRender;

use crate::{
  action::Action,
  tracer::{
    state::{BreakPoint, BreakPointPattern, BreakPointStop, BreakPointType},
    Tracer,
  },
};

use super::{help::help_item, theme::THEME};

struct BreakPointEntry {
  id: u32,
  breakpoint: BreakPoint,
  is_editing: bool,
  selected: bool,
}

impl Widget for BreakPointEntry {
  fn render(self, area: Rect, buf: &mut Buffer) {
    let space = Span::raw(" ");
    let pattern_ty = Span::styled(
      match self.breakpoint.pattern {
        BreakPointPattern::Filename(_) => " In Filename ",
        BreakPointPattern::ExactFilename(_) => " Exact Filename ",
        BreakPointPattern::ArgvRegex(_) => " Argv Regex ",
      },
      THEME.breakpoint_pattern_type_label,
    );
    let pattern = Span::styled(
      match self.breakpoint.pattern {
        BreakPointPattern::Filename(ref pattern) => pattern,
        BreakPointPattern::ExactFilename(ref pattern) => pattern.to_str().unwrap(),
        BreakPointPattern::ArgvRegex(ref _pattern) => todo!(),
      },
      THEME.breakpoint_pattern,
    );
    let line2 = Line::default().spans(vec![
      Span::styled(" Condition ", THEME.breakpoint_info_value),
      pattern_ty,
      space.clone(),
      pattern,
    ]);
    let line1 = Line::default().spans(vec![
      Span::styled(
        format!(" Breakpoint #{} ", self.id),
        if self.selected {
          THEME.breakpoint_title_selected
        } else {
          THEME.breakpoint_title
        },
      ),
      Span::styled(" Type ", THEME.breakpoint_info_label),
      Span::styled(
        match self.breakpoint.ty {
          BreakPointType::Once => "  One-Time ",
          BreakPointType::Permanent => " Permanent ",
        },
        THEME.breakpoint_info_value,
      ),
      Span::styled(" On ", THEME.breakpoint_info_label),
      Span::styled(
        match self.breakpoint.stop {
          BreakPointStop::SyscallEnter => " Syscall Enter ",
          BreakPointStop::SyscallExit => " Syscall Exit  ",
        },
        THEME.breakpoint_info_value,
      ),
      if self.breakpoint.activated {
        Span::styled("  Active  ", THEME.breakpoint_info_label_active)
      } else {
        Span::styled(" Inactive ", THEME.breakpoint_info_label)
      },
    ]);
    Paragraph::new(vec![line1, line2]).render(area, buf);
  }
}

impl PreRender for BreakPointEntry {
  fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
    self.selected = context.is_selected;
    2
  }
}

pub struct BreakPointManager;

pub struct BreakPointManagerState {
  // TODO: To support one-time breakpoints, we need to find a way to synchronize the breakpoints
  // between the tracer and the breakpoint manager. It's not trivial to store a LockGuard here.
  breakpoints: BTreeMap<u32, BreakPoint>,
  list_state: tui_widget_list::ListState,
  tracer: Arc<Tracer>,
}

impl BreakPointManagerState {
  pub fn new(tracer: Arc<Tracer>) -> Self {
    let breakpoints = tracer.get_breakpoints();
    Self {
      breakpoints,
      list_state: tui_widget_list::ListState::default().circular(true),
      tracer,
    }
  }
}

impl BreakPointManagerState {
  pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
    match key.code {
      KeyCode::Char('q') => Some(Action::CloseBreakpointManager),
      KeyCode::Down | KeyCode::Char('j') => {
        self.list_state.next();
        None
      }
      KeyCode::Up | KeyCode::Char('k') => {
        self.list_state.previous();
        None
      }
      KeyCode::Delete => {
        if let Some(selected) = self.list_state.selected {
          if selected > 0 {
            self.list_state.select(Some(selected - 1));
          } else if selected + 1 < self.breakpoints.len() {
            self.list_state.select(Some(selected + 1));
          } else {
            self.list_state.select(None);
          }
          let id = *self.breakpoints.keys().nth(selected).unwrap();
          self.tracer.remove_breakpoint(id);
          self.breakpoints.remove(&id);
        }
        None
      }
      KeyCode::Char(' ') => {
        if let Some(selected) = self.list_state.selected {
          let id = *self.breakpoints.keys().nth(selected).unwrap();
          let breakpoint = self.breakpoints.get_mut(&id).unwrap();
          self.tracer.set_breakpoint(id, !breakpoint.activated);
          breakpoint.activated = !breakpoint.activated;
        }
        None
      }
      _ => None,
    }
  }

  pub fn help(&self) -> impl Iterator<Item = Span> {
    [
      help_item!("Q", "Close Mgr"),
      help_item!("Del", "Delete"),
      help_item!("Enter", "Edit"),
      help_item!("Space", "Enable/Disable"),
      help_item!("N", "New Breakpoint"),
    ]
    .into_iter()
    .flatten()
  }
}

impl StatefulWidgetRef for BreakPointManager {
  type State = BreakPointManagerState;

  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    Clear.render(area, buf);
    let block = Block::new()
      .title(" Breakpoint Manager ")
      .borders(Borders::ALL)
      .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    block.render(area, buf);
    let list = tui_widget_list::List::new(
      state
        .breakpoints
        .iter()
        .map(|(id, breakpoint)| BreakPointEntry {
          id: *id,
          breakpoint: breakpoint.clone(),
          is_editing: false,
          selected: false,
        })
        .collect(),
    );
    if !state.breakpoints.is_empty() && state.list_state.selected.is_none() {
      state.list_state.select(Some(0));
    }
    list.render(inner, buf, &mut state.list_state);
  }
}
