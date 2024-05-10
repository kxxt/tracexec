use std::collections::BTreeSet;

use indexmap::IndexMap;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Query {
  pub kind: QueryKind,
  pub value: QueryValue,
  pub case_sensitive: bool,
}

#[derive(Debug, Clone)]
pub enum QueryValue {
  Regex(Regex),
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

  pub fn matches(&self, text: &str) -> bool {
    let result = match &self.value {
      QueryValue::Regex(re) => re.is_match(text),
      QueryValue::Text(query) => {
        if self.case_sensitive {
          text.contains(query)
        } else {
          text.to_lowercase().contains(&query.to_lowercase())
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

  pub fn statistics(&self) -> String {
    if self.indices.is_empty() {
      "No matches".to_string()
    } else {
      let total = self.indices.len();
      let selected = self.selection().map(|index| index + 1).unwrap_or(0);
      format!("{} of {}", selected, total)
    }
  }
}
