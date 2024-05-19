use std::fmt::Display;

use ratatui::text::{Line, Span};
use regex_cursor::{Cursor, IntoCursor};

#[derive(Debug, Clone)]
pub enum ValueOrRange {
  Value(Vec<String>),
  Range(usize, usize),
}

#[derive(Debug, Clone)]
pub struct EventLine {
  pub line: Line<'static>,
  pub cwd: Option<ValueOrRange>,
  pub env_range: Option<ValueOrRange>,
}

impl From<Line<'static>> for EventLine {
  fn from(line: Line<'static>) -> Self {
    Self {
      line,
      cwd: None,
      env_range: None,
    }
  }
}

impl Display for EventLine {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.line)
  }
}

trait BidirectionalIterator: Iterator {
  fn prev(&mut self) -> Option<Self::Item>;
}

impl<'a, T> Iterator for BidirectionalIter<'a, T> {
  type Item = &'a T;

  fn next(&mut self) -> Option<Self::Item> {
    self.index.next(self.slice.len());
    self.index.get(self.slice)
  }
}

impl<'a, T> BidirectionalIterator for BidirectionalIter<'a, T> {
  fn prev(&mut self) -> Option<Self::Item> {
    self.index.prev(self.slice.len());
    self.index.get(self.slice)
  }
}

enum BidirectionalIterIndex {
  Start,
  Index(usize),
  End,
}

impl BidirectionalIterIndex {
  fn next(&mut self, len: usize) {
    match self {
      Self::Start => {
        *self = if len > 0 { Self::Index(0) } else { Self::Start };
      }
      Self::Index(index) => {
        if *index + 1 < len {
          *index += 1;
        } else {
          *self = Self::End;
        }
      }
      Self::End => {}
    }
  }

  fn prev(&mut self, len: usize) {
    match self {
      Self::Start => {}
      Self::Index(index) => {
        if *index > 0 {
          *index -= 1;
        } else {
          *self = Self::Start;
        }
      }
      Self::End => {
        *self = Self::Index(len.saturating_sub(1));
      }
    }
  }

  fn get<'a, T>(&self, slice: &'a [T]) -> Option<&'a T> {
    match self {
      Self::Start => None,
      Self::Index(index) => slice.get(*index),
      Self::End => None,
    }
  }
}

struct BidirectionalIter<'a, T> {
  slice: &'a [T],
  index: BidirectionalIterIndex,
}

impl<'a, T> BidirectionalIter<'a, T> {
  fn new(slice: &'a [T]) -> Self {
    Self {
      slice,
      index: BidirectionalIterIndex::Start,
    }
  }
}

impl<T> BidirectionalIter<'_, T> {
  pub fn by_ref(&mut self) -> &mut Self {
    self
  }
}

trait IntoBidirectionalIterator {
  type Iter: BidirectionalIterator;

  fn into_bidirectional_iter(self) -> Self::Iter;
}

impl<'a, T> IntoBidirectionalIterator for &'a [T] {
  type Iter = BidirectionalIter<'a, T>;

  fn into_bidirectional_iter(self) -> Self::Iter {
    BidirectionalIter::new(self)
  }
}

// Original Copyright Notice for the following code:

// Copyright (c) 2024 Pascal Kuthe

// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:

// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorPosition {
  ChunkStart,
  ChunkEnd,
}

pub struct EventLineCursor<'a, 'b> {
  iter: BidirectionalIter<'a, Span<'b>>,
  /// Current chunk
  current: &'a [u8],
  /// Cursor position
  position: CursorPosition,
  /// Length in bytes
  len: usize,
  /// Byte offset
  offset: usize,
}

impl<'a> IntoCursor for &'a EventLine {
  type Cursor = EventLineCursor<'a, 'static>;

  fn into_cursor(self) -> Self::Cursor {
    EventLineCursor::new(self.line.spans.as_slice())
  }
}

impl<'a, 'b> EventLineCursor<'a, 'b>
where
  'b: 'a,
{
  fn new(slice: &'a [Span<'b>]) -> Self {
    let mut res = Self {
      iter: slice.into_bidirectional_iter(),
      current: &[],
      position: CursorPosition::ChunkEnd,
      len: slice.iter().map(|s| s.content.len()).sum(),
      offset: 0,
    };
    res.advance();
    res
  }
}

impl<'a, 'b> Cursor for EventLineCursor<'a, 'b>
where
  'b: 'a,
{
  fn chunk(&self) -> &[u8] {
    self.current
  }

  fn advance(&mut self) -> bool {
    match self.position {
      CursorPosition::ChunkStart => {
        self.iter.next();
        self.position = CursorPosition::ChunkEnd;
      }
      CursorPosition::ChunkEnd => (),
    }
    for next in self.iter.by_ref() {
      if next.content.is_empty() {
        continue;
      }
      self.offset += self.current.len();
      self.current = next.content.as_bytes();
      return true;
    }
    false
  }

  fn backtrack(&mut self) -> bool {
    match self.position {
      CursorPosition::ChunkStart => {}
      CursorPosition::ChunkEnd => {
        self.iter.prev();
        self.position = CursorPosition::ChunkStart;
      }
    }
    while let Some(prev) = self.iter.prev() {
      if prev.content.is_empty() {
        continue;
      }
      self.offset -= prev.content.len();
      self.current = prev.content.as_bytes();
      return true;
    }
    false
  }

  fn total_bytes(&self) -> Option<usize> {
    Some(self.len)
  }

  fn offset(&self) -> usize {
    self.offset
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use ratatui::text::Span;

  #[test]
  fn smoke_test() {
    let single = vec![Span::raw("abc")];
    let mut cursor = EventLineCursor::new(single.as_slice());
    assert_eq!(cursor.chunk(), "abc".as_bytes());
    assert!(!cursor.advance());
    assert_eq!(cursor.chunk(), "abc".as_bytes());
    assert!(!cursor.backtrack());
    assert_eq!(cursor.chunk(), "abc".as_bytes());
    let multi = vec![Span::raw("abc"); 100];
    let mut cursor = EventLineCursor::new(multi.as_slice());
    let mut offset = 0;
    loop {
      assert_eq!(cursor.offset(), offset);
      offset += cursor.chunk().len();
      if !cursor.advance() {
        break;
      }
    }
    loop {
      offset -= cursor.chunk().len();
      assert_eq!(cursor.offset(), offset);
      if !cursor.backtrack() {
        break;
      }
    }
    assert_eq!(cursor.offset(), 0);
    assert_eq!(offset, 0);
  }
}
