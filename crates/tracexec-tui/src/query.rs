use std::error::Error;

use crossterm::event::{
  KeyCode,
  KeyEvent,
  KeyModifiers,
};
use itertools::Itertools;
use ratatui::{
  style::Styled,
  text::{
    Line,
    Span,
  },
  widgets::{
    StatefulWidget,
    Widget,
  },
};
use tracexec_core::{
  cli::keys::TuiKeyBindings,
  event::EventId,
  primitives::regex::{
    IntoCursor,
    engines::pikevm,
    regex_automata::util::syntax,
  },
};
use tui_prompts::{
  State,
  TextPrompt,
  TextState,
};

use super::{
  event_line::EventLine,
  help::help_item,
  theme::Theme,
};
use crate::action::Action;

#[derive(Debug, Clone)]
pub struct Query {
  pub kind: QueryKind,
  pub value: QueryValue,
  pub case_sensitive: bool,
}

#[derive(Debug, Clone)]
pub enum QueryValue {
  Regex(pikevm::PikeVM),
  Text(String),
}

impl PartialEq for QueryValue {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Text(a), Self::Text(b)) => a == b,
      _ => false, // Can't compare Regex
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
  Search,
  Filter,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
  /// The indices of matching events and the start of the match, use IndexMap to keep the order
  pub indices: indexset::BTreeSet<EventId>,
  /// The maximum of searched id
  pub searched_id: EventId,
  /// The currently focused item in query result, an index of `indices`
  pub selection: Option<EventId>,
}

impl Query {
  pub fn new(kind: QueryKind, value: QueryValue, case_sensitive: bool) -> Self {
    Self {
      kind,
      value,
      case_sensitive,
    }
  }

  pub fn matches(&self, text: &EventLine) -> bool {
    let result = match &self.value {
      QueryValue::Regex(re) => pikevm::is_match(
        re,
        &mut pikevm::Cache::new(re),
        &mut tracexec_core::primitives::regex::Input::new(text.into_cursor()),
      ),
      QueryValue::Text(query) => {
        // FIXME: Use cursor.
        if self.case_sensitive {
          text.to_string().contains(query)
        } else {
          text
            .to_string()
            .to_lowercase()
            .contains(&query.to_lowercase())
        }
      }
    };
    if result {
      tracing::trace!("{text:?} matches: {self:?}");
    }
    result
  }
}

impl QueryResult {
  pub fn next_result(&mut self) {
    if let Some(selection) = self.selection {
      self.selection = match self.indices.range((selection + 1)..).next() {
        Some(id) => Some(*id),
        None => self.indices.first().copied(),
      }
    } else if !self.indices.is_empty() {
      self.selection = self.indices.first().copied();
    }
  }

  pub fn prev_result(&mut self) {
    if let Some(selection) = self.selection {
      self.selection = match self.indices.range(..selection).next_back() {
        Some(id) => Some(*id),
        None => self.indices.last().copied(),
      };
    } else if !self.indices.is_empty() {
      self.selection = self.indices.last().copied();
    }
  }

  /// Return the id of the currently selected event
  pub fn selection(&self) -> Option<EventId> {
    self.selection
  }

  pub fn statistics(&self, theme: &Theme) -> Line<'_> {
    if self.indices.is_empty() {
      "No match".set_style(theme.query_no_match).into()
    } else {
      let total = self
        .indices
        .len()
        .to_string()
        .set_style(theme.query_match_total_cnt);
      let selected = self
        .selection
        .map(|index| self.indices.rank(&index) + 1)
        .unwrap_or(0)
        .to_string()
        .set_style(theme.query_match_current_no);
      Line::default().spans(vec![selected, "/".into(), total])
    }
  }
}

pub struct QueryBuilder {
  kind: QueryKind,
  case_sensitive: bool,
  is_regex: bool,
  state: TextState<'static>,
  editing: bool,
}

impl QueryBuilder {
  pub fn new(kind: QueryKind) -> Self {
    Self {
      kind,
      case_sensitive: false,
      state: TextState::new(),
      editing: true,
      is_regex: false,
    }
  }

  pub fn editing(&self) -> bool {
    self.editing
  }

  pub fn edit(&mut self) {
    self.editing = true;
    self.state.focus();
  }

  /// Get the current cursor position,
  /// this should be called after render is called
  pub fn cursor(&self) -> (u16, u16) {
    self.state.cursor()
  }

  pub fn handle_key_events(
    &mut self,
    key: KeyEvent,
    keys: &TuiKeyBindings,
  ) -> Result<Option<Action>, Vec<Line<'static>>> {
    if keys.query_execute.matches(key) {
      let text = self.state.value();
      if text.is_empty() {
        return Ok(Some(Action::EndSearch));
      }
      let query = Query::new(
        self.kind,
        if self.is_regex {
          QueryValue::Regex(
            pikevm::Builder::new()
              .syntax(syntax::Config::new().case_insensitive(!self.case_sensitive))
              .build(text)
              .map_err(|e| {
                e.source()
                  .unwrap() // We are directly building it from pattern text, the source syntax error is present
                  .to_string()
                  .lines()
                  .map(|line| Line::raw(line.to_owned()))
                  .collect_vec()
              })?,
          )
        } else {
          QueryValue::Text(text.to_owned())
        },
        self.case_sensitive,
      );
      self.editing = false;
      return Ok(Some(Action::ExecuteSearch(query)));
    }
    if keys.query_cancel.matches(key) {
      return Ok(Some(Action::EndSearch));
    }
    if keys.query_toggle_case.matches(key) {
      self.case_sensitive = !self.case_sensitive;
      return Ok(None);
    }
    if keys.query_toggle_regex.matches(key) {
      self.is_regex = !self.is_regex;
      return Ok(None);
    }
    if keys.query_clear.matches(key) {
      self
        .state
        .handle_key_event(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
      return Ok(None);
    }
    self.state.handle_key_event(key);
    Ok(None)
  }
}

impl QueryBuilder {
  pub fn help(&self, keys: &TuiKeyBindings, theme: &Theme) -> Vec<Span<'_>> {
    if self.editing {
      [
        help_item!(keys.query_cancel.display(), "Cancel\u{00a0}Search", theme),
        help_item!(keys.query_execute.display(), "Execute\u{00a0}Search", theme),
        help_item!(
          keys.query_toggle_case.display(),
          if self.case_sensitive {
            "Case\u{00a0}Sensitive"
          } else {
            "Case\u{00a0}Insensitive"
          },
          theme
        ),
        help_item!(
          keys.query_toggle_regex.display(),
          if self.is_regex {
            "Regex\u{00a0}Mode"
          } else {
            "Text\u{00a0}Mode"
          },
          theme
        ),
        help_item!(keys.query_clear.display(), "Clear", theme),
      ]
      .into_iter()
      .flatten()
      .collect()
    } else {
      [
        help_item!(keys.query_next_match.display(), "Next\u{00a0}Match", theme),
        help_item!(
          keys.query_prev_match.display(),
          "Previous\u{00a0}Match",
          theme
        ),
      ]
      .into_iter()
      .flatten()
      .collect()
    }
  }
}

impl Widget for &mut QueryBuilder {
  fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
  where
    Self: Sized,
  {
    TextPrompt::new(
      match self.kind {
        QueryKind::Search => "🔍",
        QueryKind::Filter => "☔",
      }
      .into(),
    )
    .render(area, buf, &mut self.state);
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
    text::Line,
  };
  use tracexec_core::{
    cli::keys::TuiKeyBindings,
    event::EventId,
  };
  use tui_prompts::{
    FocusState,
    TextState,
  };

  use super::{
    Query,
    QueryBuilder,
    QueryKind,
    QueryResult,
    QueryValue,
    pikevm,
    syntax,
  };
  use crate::{
    action::Action,
    event_line::EventLine,
    theme::current_theme,
  };

  fn make_event_line(text: &str) -> EventLine {
    EventLine {
      line: Line::from(text.to_string()),
      cwd_mask: None,
      env_mask: None,
    }
  }

  #[test]
  fn test_query_matches_text_case() {
    let line = make_event_line("Hello World");

    let q = Query::new(
      QueryKind::Search,
      QueryValue::Text("hello".to_string()),
      false,
    );
    assert!(q.matches(&line));

    let q = Query::new(
      QueryKind::Search,
      QueryValue::Text("hello".to_string()),
      true,
    );
    assert!(!q.matches(&line));
  }

  #[test]
  fn test_query_matches_regex() {
    let line = make_event_line("abc123");
    let re = pikevm::Builder::new()
      .syntax(syntax::Config::new().case_insensitive(false))
      .build(r"\d+")
      .unwrap();
    let q = Query::new(QueryKind::Search, QueryValue::Regex(re), false);
    assert!(q.matches(&line));

    let re = pikevm::Builder::new()
      .syntax(syntax::Config::new())
      .build(r"xyz")
      .unwrap();
    let q = Query::new(QueryKind::Search, QueryValue::Regex(re), false);
    assert!(!q.matches(&line));
  }

  #[test]
  fn test_query_result_navigation() {
    let mut qr = QueryResult {
      indices: vec![1, 3, 5].into_iter().map(EventId::new).collect(),
      searched_id: EventId::new(5),
      selection: None,
    };

    assert_eq!(qr.selection(), None);
    qr.next_result();
    assert_eq!(qr.selection(), Some(EventId::new(1)));
    qr.next_result();
    assert_eq!(qr.selection(), Some(EventId::new(3)));
    qr.next_result();
    assert_eq!(qr.selection(), Some(EventId::new(5)));
    qr.next_result(); // wrap around
    assert_eq!(qr.selection(), Some(EventId::new(1)));
    qr.next_result();
    assert_eq!(qr.selection(), Some(EventId::new(3)));

    qr.prev_result();
    assert_eq!(qr.selection(), Some(EventId::new(1)));
    qr.prev_result(); // don't wrap around at start
    assert_eq!(qr.selection(), Some(EventId::new(1)));
    qr.prev_result(); // don't wrap around at start
    assert_eq!(qr.selection(), Some(EventId::new(1)));
  }

  #[test]
  fn test_query_result_statistics() {
    let qr = QueryResult {
      indices: vec![10, 20, 30].into_iter().map(EventId::new).collect(),
      searched_id: EventId::new(30),
      selection: Some(EventId::new(20)),
    };

    let line = qr.statistics(current_theme());
    let s = line.to_string();
    assert!(s.contains("2")); // selected index
    assert!(s.contains("3")); // total matches
  }

  #[test]
  fn test_query_builder_toggle_flags_and_enter() {
    let mut qb = QueryBuilder::new(QueryKind::Search);
    assert!(qb.editing());
    assert!(!qb.case_sensitive);
    assert!(!qb.is_regex);

    // Toggle case sensitivity
    let keys = TuiKeyBindings::default();
    qb.handle_key_events(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::ALT), &keys)
      .unwrap();
    assert!(qb.case_sensitive);

    // Toggle regex
    qb.handle_key_events(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT), &keys)
      .unwrap();
    assert!(qb.is_regex);

    // Enter with empty input returns EndSearch
    let mut empty_qb = QueryBuilder::new(QueryKind::Search);
    let action = empty_qb
      .handle_key_events(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &keys)
      .unwrap();
    assert!(matches!(action, Some(Action::EndSearch)));
  }

  #[test]
  fn test_query_builder_edit_and_cursor() {
    let mut qb = QueryBuilder::new(QueryKind::Search);
    qb.edit();
    assert!(qb.editing());

    let cursor = qb.cursor();
    assert_eq!(cursor, (0, 0));
  }

  #[test]
  fn test_query_builder_help() {
    let qb = QueryBuilder::new(QueryKind::Search);
    let keys = TuiKeyBindings::default();
    let help = qb.help(&keys, current_theme());
    assert!(help.iter().any(|span| span.content.contains("Esc")));
    assert!(help.iter().any(|span| span.content.contains("Enter")));
  }

  #[test]
  fn snapshot_query_builder_editing() {
    let mut qb = QueryBuilder::new(QueryKind::Search);
    qb.state = TextState::new()
      .with_focus(FocusState::Focused)
      .with_value("find me");
    let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(&mut qb, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_query_builder_non_editing() {
    let mut qb = QueryBuilder::new(QueryKind::Search);
    qb.editing = false;
    qb.state = TextState::new().with_value("done");
    let mut terminal = Terminal::new(TestBackend::new(40, 3)).unwrap();
    terminal
      .draw(|frame| {
        frame.render_widget(&mut qb, frame.area());
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }
}
