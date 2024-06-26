use std::error::Error;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use indexmap::IndexMap;
use itertools::Itertools;
use ratatui::{
  style::Styled,
  text::{Line, Span},
  widgets::{StatefulWidget, Widget},
};
use regex_cursor::{engines::pikevm, regex_automata::util::syntax, IntoCursor};
use tui_prompts::{State, TextPrompt, TextState};

use crate::action::Action;

use super::{event_line::EventLine, help::help_item, theme::THEME};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
  Search,
  Filter,
}

#[derive(Debug)]
pub struct QueryResult {
  /// The indices of matching events and the start of the match, use IndexMap to keep the order
  pub indices: IndexMap<usize, usize>,
  /// The length of all searched items, used to implement incremental query
  pub searched_len: usize,
  /// The currently focused item in query result, an index of `indices`
  pub selection: Option<usize>,
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
        &mut regex_cursor::Input::new(text.into_cursor()),
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
      if selection + 1 < self.indices.len() {
        self.selection = Some(selection + 1);
      } else {
        // If the current selection is the last one, loop back to the first one
        self.selection = Some(0)
      }
    } else if !self.indices.is_empty() {
      self.selection = Some(0);
    }
  }

  pub fn prev_result(&mut self) {
    if let Some(selection) = self.selection {
      if selection > 0 {
        self.selection = Some(selection - 1);
      } else {
        // If the current selection is the first one, loop back to the last one
        self.selection = Some(self.indices.len() - 1);
      }
    } else if !self.indices.is_empty() {
      self.selection = Some(self.indices.len() - 1);
    }
  }

  /// Return the index of the currently selected item in the event list
  pub fn selection(&self) -> Option<usize> {
    self
      .selection
      .map(|index| *self.indices.get_index(index).unwrap().0)
  }

  pub fn statistics(&self) -> Line {
    if self.indices.is_empty() {
      "No match".set_style(THEME.query_no_match).into()
    } else {
      let total = self
        .indices
        .len()
        .to_string()
        .set_style(THEME.query_match_total_cnt);
      let selected = self
        .selection
        .map(|index| index + 1)
        .unwrap_or(0)
        .to_string()
        .set_style(THEME.query_match_current_no);
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

  pub fn handle_key_events(&mut self, key: KeyEvent) -> Result<Option<Action>, Vec<Line<'static>>> {
    match (key.code, key.modifiers) {
      (KeyCode::Enter, _) => {
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
      (KeyCode::Esc, KeyModifiers::NONE) => {
        return Ok(Some(Action::EndSearch));
      }
      (KeyCode::Char('i'), KeyModifiers::ALT) => {
        self.case_sensitive = !self.case_sensitive;
      }
      (KeyCode::Char('r'), KeyModifiers::ALT) => {
        self.is_regex = !self.is_regex;
      }
      _ => {
        self.state.handle_key_event(key);
      }
    }
    Ok(None)
  }
}

impl QueryBuilder {
  pub fn help(&self) -> Vec<Span> {
    if self.editing {
      [
        help_item!("Esc", "Cancel\u{00a0}Search"),
        help_item!("Enter", "Execute\u{00a0}Search"),
        help_item!(
          "Alt+I",
          if self.case_sensitive {
            "Case\u{00a0}Sensitive"
          } else {
            "Case\u{00a0}Insensitive"
          }
        ),
        help_item!(
          "Alt+R",
          if self.is_regex {
            "Regex\u{00a0}Mode"
          } else {
            "Text\u{00a0}Mode"
          }
        ),
        help_item!("Ctrl+U", "Clear"),
      ]
      .into_iter()
      .flatten()
      .collect()
    } else {
      [
        help_item!("N", "Next\u{00a0}Match"),
        help_item!("P", "Previous\u{00a0}Match"),
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
