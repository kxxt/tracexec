use std::usize;

use arcstr::ArcStr;
///! Regex and regex-cursor related code
use regex_cursor::Cursor;

pub(crate) trait BidirectionalIterator: Iterator {
  fn prev(&mut self) -> Option<Self::Item>;
}

pub(crate) trait IntoBidirectionalIterator {
  type Iter: BidirectionalIterator;

  fn into_bidirectional_iter(self) -> Self::Iter;
}

pub(crate) struct BidirectionalIter<'a, T> {
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

impl<'a, T> IntoBidirectionalIterator for &'a [T] {
  type Iter = BidirectionalIter<'a, T>;

  fn into_bidirectional_iter(self) -> Self::Iter {
    BidirectionalIter::new(self)
  }
}

enum BidirectionalInterspersedIterIndex {
  Start,
  Index(usize),
  // The separator after the index
  Separator(usize),
  End,
}

pub(crate) struct BidirectionalInterspersedIter<'a, T> {
  slice: &'a [T],
  index: BidirectionalInterspersedIterIndex,
  separator: &'a T,
}

impl<'a, T> BidirectionalInterspersedIter<'a, T> {
  fn new(slice: &'a [T], separator: &'a T) -> Self {
    Self {
      slice,
      index: BidirectionalInterspersedIterIndex::Start,
      separator,
    }
  }
}

impl BidirectionalInterspersedIterIndex {
  fn next(&mut self, len: usize) {
    match self {
      Self::Start => {
        *self = if len > 0 { Self::Index(0) } else { Self::Start };
      }
      Self::Index(index) => {
        if *index + 1 < len {
          // Not last one
          *self = Self::Separator(*index)
        } else {
          *self = Self::End;
        }
      }
      Self::End => {}
      Self::Separator(index) => *self = Self::Index(*index + 1),
    }
  }

  fn prev(&mut self, len: usize) {
    match self {
      Self::Start => {}
      Self::Index(index) => {
        if *index > 0 {
          // Not first one
          *self = Self::Separator(*index - 1)
        } else {
          *self = Self::Start;
        }
      }
      Self::End => {
        if len > 0 {
          *self = Self::Index(len.saturating_sub(1));
        } else {
          *self = Self::Start;
        }
      }
      Self::Separator(index) => *self = Self::Index(*index),
    }
  }

  fn get<'a, T>(&self, slice: &'a [T], separator: &'a T) -> Option<&'a T> {
    match self {
      Self::Start => None,
      Self::Index(index) => slice.get(*index),
      Self::Separator(_) => Some(separator),
      Self::End => None,
    }
  }
}

impl<'a, T> Iterator for BidirectionalInterspersedIter<'a, T> {
  type Item = &'a T;

  fn next(&mut self) -> Option<Self::Item> {
    self.index.next(self.slice.len());
    self.index.get(self.slice, self.separator)
  }
}

impl<'a, T> BidirectionalIterator for BidirectionalInterspersedIter<'a, T> {
  fn prev(&mut self) -> Option<Self::Item> {
    self.index.prev(self.slice.len());
    self.index.get(self.slice, self.separator)
  }
}

#[cfg(test)]
mod iter_tests {
  use super::BidirectionalInterspersedIter;
  use crate::regex::BidirectionalIterator;

  #[test]
  fn biter_interspersed_roundtrip() {
    let slice = [1, 2, 3, 4];
    let separator = 0;
    let mut biter = BidirectionalInterspersedIter::new(&slice, &separator);
    assert_eq!(biter.next(), Some(&1));
    assert_eq!(biter.next(), Some(&0));
    assert_eq!(biter.next(), Some(&2));
    assert_eq!(biter.next(), Some(&0));
    assert_eq!(biter.next(), Some(&3));
    assert_eq!(biter.next(), Some(&0));
    assert_eq!(biter.next(), Some(&4));
    assert_eq!(biter.next(), None);
    assert_eq!(biter.prev(), Some(&4));
    assert_eq!(biter.prev(), Some(&0));
    assert_eq!(biter.prev(), Some(&3));
    assert_eq!(biter.prev(), Some(&0));
    assert_eq!(biter.prev(), Some(&2));
    assert_eq!(biter.prev(), Some(&0));
    assert_eq!(biter.prev(), Some(&1));
    assert_eq!(biter.prev(), None);
  }

  #[test]
  fn biter_interspersed_two_items() {
    let slice = [1, 2];
    let separator = 0;
    let mut biter = BidirectionalInterspersedIter::new(&slice, &separator);
    assert_eq!(biter.next(), Some(&1));
    assert_eq!(biter.next(), Some(&0));
    assert_eq!(biter.next(), Some(&2));
    assert_eq!(biter.prev(), Some(&0));
    assert_eq!(biter.next(), Some(&2));
    assert_eq!(biter.next(), None);
    assert_eq!(biter.next(), None);
    assert_eq!(biter.prev(), Some(&2));
    assert_eq!(biter.prev(), Some(&0));
    assert_eq!(biter.prev(), Some(&1));
    assert_eq!(biter.prev(), None);
    assert_eq!(biter.prev(), None);
    assert_eq!(biter.next(), Some(&1));
    assert_eq!(biter.next(), Some(&0));
    assert_eq!(biter.prev(), Some(&1));
  }

  #[test]
  fn biter_interspersed_single_item() {
    let slice = [1];
    let separator = 0;
    let mut biter = BidirectionalInterspersedIter::new(&slice, &separator);
    assert_eq!(biter.next(), Some(&1));
    assert_eq!(biter.next(), None);
    assert_eq!(biter.next(), None);
    assert_eq!(biter.prev(), Some(&1));
    assert_eq!(biter.prev(), None);
  }
}

pub struct Argv<'a>(&'a [&'a ArcStr]);

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

/// Argv joined by a single white space between arguments
pub struct ArgvCursor<'a> {
  iter: BidirectionalInterspersedIter<'a, ArcStr>,
  current: &'a [u8],
  position: CursorPosition,
  len: usize,
  offset: usize,
}

const SPACE: ArcStr = arcstr::literal!(" ");

impl<'a> ArgvCursor<'a> {
  fn new(slice: &'a [ArcStr], separator: &'a ArcStr) -> Self {
    let mut res = Self {
      iter: BidirectionalInterspersedIter::new(slice, separator),
      current: &[],
      position: CursorPosition::ChunkEnd,
      len: slice.iter().map(|s| s.len()).sum::<usize>()
        + (slice.len().saturating_sub(1)) * separator.len(),
      offset: 0,
    };
    res.advance();
    res
  }
}

impl<'a> Cursor for ArgvCursor<'a> {
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
      if next.is_empty() {
        continue;
      }
      self.offset += self.current.len();
      self.current = next.as_bytes();
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
      if prev.is_empty() {
        continue;
      }
      self.offset -= prev.len();
      self.current = prev.as_bytes();
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
mod cursor_tests {
  use super::*;

  #[test]
  fn smoke_test() {
    let single = vec![ArcStr::from("abc")];
    let separator = SPACE;
    let mut cursor = ArgvCursor::new(single.as_slice(), &separator);
    assert_eq!(cursor.chunk(), "abc".as_bytes());
    assert!(!cursor.advance());
    assert_eq!(cursor.chunk(), "abc".as_bytes());
    assert!(!cursor.backtrack());
    assert_eq!(cursor.chunk(), "abc".as_bytes());
    // 2
    let two = vec![ArcStr::from("abc"); 2];
    let mut cursor = ArgvCursor::new(two.as_slice(), &separator);
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
    // multi
    let multi = vec![ArcStr::from("abc"); 100];
    let mut cursor = ArgvCursor::new(multi.as_slice(), &separator);
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
