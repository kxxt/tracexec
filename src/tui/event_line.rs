use std::{borrow::Cow, fmt::Display, mem, ops::Range};

use ratatui::text::{Line, Span};
use regex_cursor::{Cursor, IntoCursor};

use crate::regex::{BidirectionalIter, BidirectionalIterator, IntoBidirectionalIterator};

#[derive(Debug, Clone)]
pub struct Mask {
  /// The range of the spans to mask
  pub range: Range<usize>,
  /// The value of the spans to mask
  pub values: Vec<Cow<'static, str>>,
}

impl Mask {
  pub fn new(range: Range<usize>) -> Self {
    Self {
      values: vec![Default::default(); range.len()],
      range,
    }
  }

  pub fn toggle(&mut self, line: &mut Line<'static>) {
    for (span, value) in line.spans[self.range.clone()]
      .iter_mut()
      .zip(self.values.iter_mut())
    {
      mem::swap(&mut span.content, value);
    }
  }
}

#[derive(Debug, Clone)]
pub struct EventLine {
  pub line: Line<'static>,
  pub cwd_mask: Option<Mask>,
  pub env_mask: Option<Mask>,
}

impl From<Line<'static>> for EventLine {
  fn from(line: Line<'static>) -> Self {
    Self {
      line,
      cwd_mask: None,
      env_mask: None,
    }
  }
}

impl Display for EventLine {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.line)
  }
}

impl EventLine {
  pub fn toggle_cwd_mask(&mut self) {
    if let Some(mask) = &mut self.cwd_mask {
      mask.toggle(&mut self.line);
    }
  }

  pub fn toggle_env_mask(&mut self) {
    if let Some(mask) = &mut self.env_mask {
      mask.toggle(&mut self.line);
    }
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
    assert_eq!(cursor.chunk(), b"abc");
    assert!(!cursor.advance());
    assert_eq!(cursor.chunk(), b"abc");
    assert!(!cursor.backtrack());
    assert_eq!(cursor.chunk(), b"abc");
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
