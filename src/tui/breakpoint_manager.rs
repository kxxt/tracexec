use std::{collections::BTreeMap, sync::Arc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
  layout::{Alignment, Constraint, Layout},
  prelude::{Buffer, Rect},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, StatefulWidgetRef, Widget, Wrap},
};
use tui_prompts::{State, TextPrompt, TextState};
use tui_widget_list::PreRender;

use crate::{
  action::Action,
  tracer::{
    state::{BreakPoint, BreakPointPattern, BreakPointStop, BreakPointType},
    Tracer,
  },
};

use super::{error_popup::ErrorPopupState, help::help_item, theme::THEME};

struct BreakPointEntry {
  id: u32,
  breakpoint: BreakPoint,
  selected: bool,
}

impl BreakPointEntry {
  fn paragraph(&self) -> Paragraph {
    let space = Span::raw(" ");
    let pattern_ty = Span::styled(
      match self.breakpoint.pattern {
        BreakPointPattern::Filename(_) => "\u{00a0}In\u{00a0}Filename\u{00a0}\u{200b}",
        BreakPointPattern::ExactFilename(_) => "\u{00a0}Exact\u{00a0}Filename\u{00a0}\u{200b}",
        BreakPointPattern::ArgvRegex(_) => "\u{00a0}Argv\u{00a0}Regex\u{00a0}\u{200b}",
      },
      THEME.breakpoint_pattern_type_label,
    );
    let pattern = Span::styled(self.breakpoint.pattern.pattern(), THEME.breakpoint_pattern);
    let line2 = Line::default().spans(vec![
      Span::styled(
        "\u{00a0}Condition\u{00a0}\u{200b}",
        THEME.breakpoint_info_value,
      ),
      pattern_ty,
      space.clone(),
      pattern,
    ]);
    let line1 = Line::default().spans(vec![
      Span::styled(
        format!("\u{00a0}Breakpoint\u{00a0}#{}\u{00a0}\u{200b}", self.id),
        if self.selected {
          THEME.breakpoint_title_selected
        } else {
          THEME.breakpoint_title
        },
      ),
      Span::styled("\u{00a0}Type\u{00a0}", THEME.breakpoint_info_label),
      Span::styled(
        match self.breakpoint.ty {
          BreakPointType::Once => "\u{00a0}\u{00a0}One-Time\u{00a0}\u{200b}",
          BreakPointType::Permanent => "\u{00a0}Permanent\u{00a0}\u{200b}",
        },
        THEME.breakpoint_info_value,
      ),
      Span::styled("\u{00a0}On\u{00a0}", THEME.breakpoint_info_label),
      Span::styled(
        match self.breakpoint.stop {
          BreakPointStop::SyscallEnter => "\u{00a0}Syscall\u{00a0}Enter\u{00a0}\u{200b}",
          BreakPointStop::SyscallExit => "\u{00a0}Syscall\u{00a0}\u{00a0}Exit\u{00a0}\u{200b}",
        },
        THEME.breakpoint_info_value,
      ),
      if self.breakpoint.activated {
        Span::styled(
          "\u{00a0}\u{00a0}Active\u{00a0}\u{00a0}",
          THEME.breakpoint_info_label_active,
        )
      } else {
        Span::styled("\u{00a0}Inactive\u{00a0}", THEME.breakpoint_info_label)
      },
    ]);
    Paragraph::new(vec![line1, line2]).wrap(Wrap { trim: false })
  }
}

impl Widget for BreakPointEntry {
  fn render(self, area: Rect, buf: &mut Buffer) {
    self.paragraph().render(area, buf);
  }
}

impl PreRender for BreakPointEntry {
  fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
    self.selected = context.is_selected;
    self
      .paragraph()
      .line_count(context.cross_axis_size)
      .try_into()
      .unwrap_or(u16::MAX)
  }
}

pub struct BreakPointManager;

pub struct BreakPointManagerState {
  // TODO: To support one-time breakpoints, we need to find a way to synchronize the breakpoints
  // between the tracer and the breakpoint manager. It's not trivial to store a LockGuard here.
  breakpoints: BTreeMap<u32, BreakPoint>,
  list_state: tui_widget_list::ListState,
  tracer: Arc<Tracer>,
  editor: Option<tui_prompts::TextState<'static>>,
  /// The stop for the currently editing breakpoint
  stop: BreakPointStop,
  // Whether to activate the breakpoint being edited
  active: bool,
  editing: Option<u32>,
}

impl BreakPointManagerState {
  pub fn new(tracer: Arc<Tracer>) -> Self {
    let breakpoints = tracer.get_breakpoints();
    Self {
      breakpoints,
      list_state: tui_widget_list::ListState::default().circular(true),
      tracer,
      editor: None,
      stop: BreakPointStop::SyscallExit,
      active: true,
      editing: None,
    }
  }
}

impl BreakPointManagerState {
  pub fn cursor(&self) -> Option<(u16, u16)> {
    self.editor.as_ref().map(|editing| editing.cursor())
  }

  pub fn clear_editor(&mut self) {
    self.editor = None;
    self.editing = None;
  }

  pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
    if let Some(editor) = self.editor.as_mut() {
      match key.code {
        KeyCode::Enter => {
          if editor.is_empty() {
            self.clear_editor();
            return None;
          }
          let pattern = match BreakPointPattern::from_editable(editor.value()) {
            Ok(pattern) => pattern,
            Err(message) => {
              return Some(Action::SetActivePopup(
                crate::action::ActivePopup::ErrorPopup(ErrorPopupState {
                  title: "Breakpoint Editor Error".to_string(),
                  message: vec![Line::from(message)],
                }),
              ))
            }
          };
          let new = BreakPoint {
            pattern,
            ty: BreakPointType::Permanent,
            activated: self.active,
            stop: self.stop,
          };
          // Finish editing
          if let Some(id) = self.editing {
            // Existing breakpoint
            self.breakpoints.insert(id, new.clone());
            self.tracer.replace_breakpoint(id, new);
          } else {
            // New Breakpoint
            let id = self.tracer.add_breakpoint(new.clone());
            self.breakpoints.insert(id, new);
          }
          self.clear_editor();
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::ALT => {
          self.stop.toggle();
        }
        KeyCode::Char('a') if key.modifiers == KeyModifiers::ALT => {
          self.active = !self.active;
        }
        _ => {
          editor.handle_key_event(key);
        }
      }
      return None;
    }
    if key.modifiers == KeyModifiers::NONE {
      match key.code {
        KeyCode::Char('q') => return Some(Action::CloseBreakpointManager),
        KeyCode::Down | KeyCode::Char('j') => {
          self.list_state.next();
        }
        KeyCode::Up | KeyCode::Char('k') => {
          self.list_state.previous();
        }
        KeyCode::Delete | KeyCode::Char('d') => {
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
        }
        KeyCode::Char(' ') => {
          if let Some(selected) = self.list_state.selected {
            let id = *self.breakpoints.keys().nth(selected).unwrap();
            let breakpoint = self.breakpoints.get_mut(&id).unwrap();
            self.tracer.set_breakpoint(id, !breakpoint.activated);
            breakpoint.activated = !breakpoint.activated;
          }
        }
        KeyCode::Enter => {
          if let Some(selected) = self.list_state.selected {
            let id = *self.breakpoints.keys().nth(selected).unwrap();
            let breakpoint = self.breakpoints.get(&id).unwrap();
            self.stop = breakpoint.stop;
            self.active = breakpoint.activated;
            self.editing = Some(id);
            self.editor = Some(TextState::new().with_value(breakpoint.pattern.to_editable()));
          }
        }
        KeyCode::Char('n') => {
          self.editor = Some(TextState::new());
          self.stop = BreakPointStop::SyscallExit;
          self.active = true;
        }
        _ => {}
      }
    }
    None
  }

  pub fn help(&self) -> impl Iterator<Item = Span> {
    [
      help_item!("Q", "Close Mgr"),
      help_item!("Del/D", "Delete"),
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
    let editor_area = Rect {
      x: 0,
      y: 1,
      width: buf.area.width,
      height: 1,
    };
    Clear.render(editor_area, buf);
    if let Some(ref mut editing) = state.editor {
      let toggles_area = Rect {
        x: buf.area.width.saturating_sub(39),
        y: 0,
        width: 39.min(buf.area.width),
        height: 1,
      };
      Clear.render(toggles_area, buf);
      let [stop_toggle_area, active_toggle_area] =
        Layout::horizontal([Constraint::Length(22), Constraint::Length(17)]).areas(toggles_area);
      TextPrompt::new("🐛".into()).render(editor_area, buf, editing);
      Line::default()
        .spans(help_item!(
          "Alt+S", // 5 + 2 = 7
          match state.stop {
            BreakPointStop::SyscallEnter => "Syscall Enter", // 13 + 2 = 15
            BreakPointStop::SyscallExit => "Syscall  Exit",
          }
        ))
        .render(stop_toggle_area, buf);
      Line::default()
        .spans(help_item!(
          "Alt+A", // 5 + 2 = 7
          match state.active {
            true => " Active ", // 8 + 2 = 10
            false => "Inactive",
          }
        ))
        .render(active_toggle_area, buf);
    }
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
