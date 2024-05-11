use std::borrow::Cow;

use ratatui::text::Line;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub trait PartialLine<'a> {
  fn substring(self, start: usize, len: u16) -> Line<'a>;
}

impl<'a> PartialLine<'a> for Line<'a> {
  // Unicode is hard
  fn substring(mut self, start: usize, len: u16) -> Line<'a> {
    let len = len as usize;
    let end = start + len;
    let end = if end > self.width() {
      self.width()
    } else {
      end
    };
    let mut cur = 0;
    let mut discard_until = 0;
    let mut discard_after = self.spans.len();
    for (i, span) in self.spans.iter_mut().enumerate() {
      let span_width = span.width();
      if cur + span_width < start {
        // Discard this hidden span
        cur += span_width;
        discard_until = i + 1;
        continue;
      }
      if cur >= end {
        // Discard all following spans because we have already covered all visible spans
        discard_after = i;
        break;
      }
      // The start and end defined by width
      let start = if cur < start { start - cur } else { 0 };
      let end = if cur + span_width > end {
        end - cur
      } else {
        span_width
      };
      let mut start_index = 0;
      let mut end_index = span.content.len(); // exclusive
      let mut cum = 0; // cumulative width
      for (idx, grapheme) in span.content.grapheme_indices(true) {
        let grapheme_width = grapheme.width();
        if cum + grapheme_width < start {
          start_index = idx + grapheme.len();
          cum += grapheme_width;
        } else if start != 0 {
          // Skip the grapheme that doesn't fit in the start
          start_index = idx + grapheme.len();
          break;
        }
      }
      cum = span_width;
      for (idx, grapheme) in span.content.grapheme_indices(true).rev() {
        let grapheme_width = grapheme.width();
        if cum - grapheme_width >= end {
          end_index = idx;
          cum -= grapheme_width;
        } else {
          break;
        }
      }
      match span.content {
        Cow::Borrowed(s) => {
          span.content = Cow::Borrowed(&s[start_index..end_index]);
        }
        Cow::Owned(ref mut s) => {
          s.drain(end_index..);
          s.drain(..start_index);
        }
      }
      cur += span_width;
    }
    self.spans.drain(discard_after..);
    self.spans.drain(..discard_until);

    self
  }
}
