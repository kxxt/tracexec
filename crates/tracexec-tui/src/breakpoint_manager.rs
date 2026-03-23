use std::collections::BTreeMap;

use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use itertools::{
  Itertools,
  chain,
};
use ratatui::{
  layout::{
    Alignment,
    Constraint,
    Layout,
  },
  prelude::{
    Buffer,
    Rect,
  },
  style::Stylize,
  text::{
    Line,
    Span,
  },
  widgets::{
    Block,
    Borders,
    Clear,
    Paragraph,
    StatefulWidget,
    StatefulWidgetRef,
    Widget,
    Wrap,
  },
};
use tracexec_backend_ptrace::ptrace::RunningTracer;
use tracexec_core::breakpoint::{
  BreakPoint,
  BreakPointPattern,
  BreakPointStop,
  BreakPointType,
};
use tui_prompts::{
  State,
  TextPrompt,
  TextState,
};
use tui_widget_list::{
  ListBuilder,
  ListView,
};

use super::{
  error_popup::InfoPopupState,
  help::{
    help_item,
    help_key,
  },
  theme::THEME,
};
use crate::action::{
  Action,
  ActivePopup,
};

struct BreakPointEntry {
  id: u32,
  breakpoint: BreakPoint,
}

impl BreakPointEntry {
  fn paragraph(&self, selected: bool) -> Paragraph<'static> {
    let space = Span::raw(" ");
    let pattern_ty = Span::styled(
      match self.breakpoint.pattern {
        BreakPointPattern::InFilename(_) => "\u{00a0}In\u{00a0}Filename\u{00a0}\u{200b}",
        BreakPointPattern::ExactFilename(_) => "\u{00a0}Exact\u{00a0}Filename\u{00a0}\u{200b}",
        BreakPointPattern::ArgvRegex(_) => "\u{00a0}Argv\u{00a0}Regex\u{00a0}\u{200b}",
      },
      THEME.breakpoint_pattern_type_label,
    );
    let pattern = Span::styled(
      self.breakpoint.pattern.pattern().to_owned(),
      THEME.breakpoint_pattern,
    );
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
        if selected {
          THEME.breakpoint_title_selected
        } else {
          THEME.breakpoint_title
        },
      ),
      Span::styled("\u{00a0}Type\u{00a0}\u{200b}", THEME.breakpoint_info_label),
      Span::styled(
        match self.breakpoint.ty {
          BreakPointType::Once => "\u{00a0}\u{00a0}One-Time\u{00a0}\u{200b}",
          BreakPointType::Permanent => "\u{00a0}Permanent\u{00a0}\u{200b}",
        },
        THEME.breakpoint_info_value,
      ),
      Span::styled("\u{00a0}On\u{00a0}\u{200b}", THEME.breakpoint_info_label),
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

pub struct BreakPointManager;

pub struct BreakPointManagerState {
  // TODO: To support one-time breakpoints, we need to find a way to synchronize the breakpoints
  // between the tracer and the breakpoint manager. It's not trivial to store a LockGuard here.
  breakpoints: BTreeMap<u32, BreakPoint>,
  list_state: tui_widget_list::ListState,
  tracer: RunningTracer,
  editor: Option<tui_prompts::TextState<'static>>,
  /// The stop for the currently editing breakpoint
  stop: BreakPointStop,
  // Whether to activate the breakpoint being edited
  active: bool,
  editing: Option<u32>,
}

impl BreakPointManager {
  fn help() -> InfoPopupState {
    InfoPopupState::info(
      "How to use Breakpoints".to_string(),
      vec![
        Line::default().spans(vec![
          "Breakpoint in tracexec is similar to breakpoints in debuggers. \
        But instead of setting breakpoint on code lines, you set breakpoints on program exec. \
        The breakpoints can be set on "
            .into(),
          "syscall-enter(right before exec)".cyan().bold(),
          " or ".into(),
          "syscall-exit(right after exec)".cyan().bold(),
          " stops. There are three kinds of breakpoint patterns now:".into(),
        ]),
        Line::default().spans(vec![
          "1. ".cyan().bold(),
          "in-filename".red().bold(),
          ": Break when the filename contains the pattern string".into(),
        ]),
        Line::default().spans(vec![
          "2. ".cyan().bold(),
          "exact-filename".red().bold(),
          ": Break when the filename is exactly the same as the pattern string".into(),
        ]),
        Line::default().spans(vec![
          "3. ".cyan().bold(),
          "argv-regex".red().bold(),
          ": Break when the argv(joined by whitespace without escaping) contains the pattern regex"
            .into(),
        ]),
        Line::default().spans(vec![
          "Press ".into(),
          help_key("N"),
          " to create a new breakpoint. Press ".into(),
          help_key("Enter/E"),
          " when a breakpoint is selected to edit it. \
          While editing a breakpoint, the editor is shown on top of the screen. \
          The editor accepts breakpoint pattern in the following format: "
            .into(),
          "pattern-kind".red().bold().italic(),
          ":".black().bold(),
          "pattern-string".cyan().bold().italic(),
          " where the ".into(),
          "pattern-kind".red().bold().italic(),
          " is one of the three kinds mentioned above highlighted in red. \
          And do note that there's no space between the colon and the "
            .into(),
          "pattern-string".cyan().bold().italic(),
          ". To change the breakpoint stop or disable the breakpoint, please follow on screen instructions. \
          Press ".into(),
          help_key("Enter"),
          " to save the breakpoint. Press ".into(),
          help_key("Ctrl+C"),
          " to cancel the editing.".into(),
        ]),
        Line::default().spans(vec![
          "When an exec event hit a breakpoint, the corresponding process is stopped. \
          It is highlighted in the bottom of the screen and you can follow the instructions to manage stopped processes.",
        ]),
      ],
    )
  }
}

#[cfg(test)]
mod tests {
  use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
  };
  use insta::assert_snapshot;
  use ratatui::{
    Terminal,
    backend::TestBackend,
  };
  use tracexec_backend_ptrace::ptrace::RunningTracer;
  use tracexec_core::breakpoint::{
    BreakPoint,
    BreakPointPattern,
    BreakPointStop,
    BreakPointType,
  };

  use super::{
    BreakPointEntry,
    BreakPointManagerState,
  };

  fn make_breakpoint(pattern: &str, stop: BreakPointStop, activated: bool) -> BreakPoint {
    BreakPoint {
      pattern: BreakPointPattern::from_editable(pattern).unwrap(),
      ty: BreakPointType::Permanent,
      activated,
      stop,
    }
  }

  fn feed_text(state: &mut BreakPointManagerState, text: &str) {
    for ch in text.chars() {
      state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
  }

  #[test]
  fn snapshot_breakpoint_entry() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::InFilename("/bin/echo".to_string()),
        ty: BreakPointType::Permanent,
        activated: true,
        stop: BreakPointStop::SyscallEnter,
      },
    };
    let paragraph = entry.paragraph(true);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_breakpoint_entry_unselected() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::InFilename("/bin/echo".to_string()),
        ty: BreakPointType::Permanent,
        activated: true,
        stop: BreakPointStop::SyscallEnter,
      },
    };
    let paragraph = entry.paragraph(false);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_breakpoint_entry_inactive() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::InFilename("/bin/echo".to_string()),
        ty: BreakPointType::Permanent,
        activated: false,
        stop: BreakPointStop::SyscallEnter,
      },
    };
    let paragraph = entry.paragraph(true);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_breakpoint_entry_once_type() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::InFilename("/bin/echo".to_string()),
        ty: BreakPointType::Once,
        activated: true,
        stop: BreakPointStop::SyscallEnter,
      },
    };
    let paragraph = entry.paragraph(true);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_breakpoint_entry_syscall_exit() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::InFilename("/bin/echo".to_string()),
        ty: BreakPointType::Permanent,
        activated: true,
        stop: BreakPointStop::SyscallExit,
      },
    };
    let paragraph = entry.paragraph(true);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_breakpoint_entry_exact_filename() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::ExactFilename("/bin/echo".to_string()),
        ty: BreakPointType::Permanent,
        activated: true,
        stop: BreakPointStop::SyscallEnter,
      },
    };
    let paragraph = entry.paragraph(true);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_breakpoint_entry_argv_regex() {
    let entry = BreakPointEntry {
      id: 7,
      breakpoint: BreakPoint {
        pattern: BreakPointPattern::from_editable("argv-regex:curl.*google").unwrap(),
        ty: BreakPointType::Permanent,
        activated: true,
        stop: BreakPointStop::SyscallEnter,
      },
    };
    let paragraph = entry.paragraph(true);
    let mut terminal = Terminal::new(TestBackend::new(70, 4)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(paragraph, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn test_breakpoint_manager_help() {
    let help = super::BreakPointManager::help();
    assert_eq!(help.title, "How to use Breakpoints");
    assert!(!help.message.is_empty());
    // Check that the help contains expected content
    let content = help
      .message
      .iter()
      .map(|line: &ratatui::text::Line| line.to_string())
      .collect::<Vec<_>>()
      .join("\n");
    assert!(content.contains("syscall-enter"));
    assert!(content.contains("syscall-exit"));
    assert!(content.contains("in-filename"));
    assert!(content.contains("exact-filename"));
    assert!(content.contains("argv-regex"));
  }

  #[test]
  fn test_breakpoint_manager_state_new_copies_breakpoints() {
    let tracer = RunningTracer::mock();
    let id = tracer.add_breakpoint(make_breakpoint(
      "in-filename:/bin/echo",
      BreakPointStop::SyscallEnter,
      true,
    ));
    let state = BreakPointManagerState::new(tracer.clone());
    assert_eq!(state.breakpoints.len(), 1);
    let breakpoint = state.breakpoints.get(&id).unwrap().clone();
    assert_eq!(breakpoint.pattern.to_editable(), "in-filename:/bin/echo");
    assert_eq!(breakpoint.stop, BreakPointStop::SyscallEnter);
    assert!(breakpoint.activated);
    assert!(matches!(breakpoint.ty, BreakPointType::Permanent));
  }

  #[test]
  fn test_breakpoint_manager_state_new_breakpoint_flow() {
    let tracer = RunningTracer::mock();
    let mut state = BreakPointManagerState::new(tracer.clone());
    state.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    feed_text(&mut state, "in-filename:/bin/echo");
    state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(state.editor.is_none());
    assert_eq!(state.breakpoints.len(), 1);
    let (id, breakpoint) = state.breakpoints.iter().next().unwrap();
    let breakpoint = breakpoint.clone();
    assert_eq!(breakpoint.pattern.to_editable(), "in-filename:/bin/echo");
    assert_eq!(breakpoint.stop, BreakPointStop::SyscallExit);
    assert!(breakpoint.activated);
    assert!(matches!(breakpoint.ty, BreakPointType::Permanent));
    let tracer_breakpoints = tracer.get_breakpoints();
    assert!(tracer_breakpoints.contains_key(id));
  }

  #[test]
  fn test_breakpoint_manager_state_edit_toggles_and_saves() {
    let tracer = RunningTracer::mock();
    let id = tracer.add_breakpoint(make_breakpoint(
      "in-filename:/bin/echo",
      BreakPointStop::SyscallExit,
      true,
    ));
    let mut state = BreakPointManagerState::new(tracer.clone());
    state.list_state.select(Some(0));
    state.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    state.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
    state.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let breakpoint = state.breakpoints.get(&id).unwrap();
    assert_eq!(breakpoint.stop, BreakPointStop::SyscallEnter);
    assert!(!breakpoint.activated);
    let tracer_breakpoints = tracer.get_breakpoints();
    let tracer_breakpoint = tracer_breakpoints.get(&id).unwrap();
    assert_eq!(tracer_breakpoint.stop, BreakPointStop::SyscallEnter);
    assert!(!tracer_breakpoint.activated);
  }

  #[test]
  fn test_breakpoint_manager_state_delete_breakpoint() {
    let tracer = RunningTracer::mock();
    let id1 = tracer.add_breakpoint(make_breakpoint(
      "in-filename:/bin/echo",
      BreakPointStop::SyscallExit,
      true,
    ));
    let id2 = tracer.add_breakpoint(make_breakpoint(
      "exact-filename:/bin/sleep",
      BreakPointStop::SyscallEnter,
      true,
    ));
    let mut state = BreakPointManagerState::new(tracer.clone());
    state.list_state.select(Some(1));
    state.handle_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(state.breakpoints.len(), 1);
    assert!(state.breakpoints.contains_key(&id1));
    assert!(!state.breakpoints.contains_key(&id2));
    assert_eq!(state.list_state.selected, Some(0));
    let tracer_breakpoints = tracer.get_breakpoints();
    assert!(tracer_breakpoints.contains_key(&id1));
    assert!(!tracer_breakpoints.contains_key(&id2));
  }
}

impl BreakPointManagerState {
  pub fn new(tracer: RunningTracer) -> Self {
    let breakpoints = tracer.get_breakpoints();
    Self {
      breakpoints,
      list_state: tui_widget_list::ListState::default(),
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
              return Some(Action::SetActivePopup(ActivePopup::InfoPopup(
                InfoPopupState::error(
                  "Breakpoint Editor Error".to_string(),
                  vec![Line::from(message)],
                ),
              )));
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
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
          self.clear_editor();
        }
        _ => {
          editor.handle_key_event(key);
        }
      }
      return None;
    }
    if key.modifiers == KeyModifiers::NONE {
      match key.code {
        KeyCode::F(1) => {
          return Some(Action::SetActivePopup(ActivePopup::InfoPopup(
            BreakPointManager::help(),
          )));
        }
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
        KeyCode::Char('e') | KeyCode::Enter => {
          if let Some(selected) = self.list_state.selected {
            let id = *self.breakpoints.keys().nth(selected).unwrap();
            let breakpoint = self.breakpoints.get(&id).unwrap();
            self.stop = breakpoint.stop;
            self.active = breakpoint.activated;
            self.editing = Some(id);
            let mut editor_state = TextState::new().with_value(breakpoint.pattern.to_editable());
            editor_state.move_end();
            self.editor = Some(editor_state);
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

  pub fn help(&self) -> impl Iterator<Item = Span<'_>> {
    chain!(
      [
        help_item!("Q", "Close Mgr"),
        help_item!("Del/D", "Delete"),
        help_item!("Enter/E", "Edit"),
        help_item!("Space", "Enable/Disable"),
        help_item!("N", "New\u{00a0}Breakpoint"),
      ],
      if self.editor.is_some() {
        Some(help_item!("Ctrl+C", "Cancel"))
      } else {
        None
      }
    )
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
    } else {
      let help_area = Rect {
        x: buf.area.width.saturating_sub(10),
        y: 0,
        width: 10.min(buf.area.width),
        height: 1,
      };
      Clear.render(help_area, buf);
      Line::default()
        .spans(help_item!("F1", "Help"))
        .render(help_area, buf);
    }
    Clear.render(area, buf);
    let block = Block::new()
      .title(" Breakpoint Manager ")
      .borders(Borders::ALL)
      .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    block.render(area, buf);
    let items = state
      .breakpoints
      .iter()
      .map(|(id, breakpoint)| BreakPointEntry {
        id: *id,
        breakpoint: breakpoint.clone(),
      })
      .collect_vec();
    let builder = ListBuilder::new(move |ctx| {
      let item = &items[ctx.index];
      let paragraph = item.paragraph(ctx.is_selected);
      let line_count = paragraph
        .line_count(ctx.cross_axis_size)
        .try_into()
        .unwrap_or(u16::MAX);
      (paragraph, line_count)
    });
    let list = ListView::new(builder, state.breakpoints.len());
    if !state.breakpoints.is_empty() && state.list_state.selected.is_none() {
      state.list_state.select(Some(0));
    }
    list.render(inner, buf, &mut state.list_state);
  }
}
