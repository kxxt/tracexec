use std::borrow::Cow;

use ratatui::text::Line;

pub trait PartialLine<'a> {
    fn substring(self, start: usize, len: u16) -> Line<'a>;
    fn truncate_start(self, start: usize) -> Line<'a>;
}

impl<'a> PartialLine<'a> for Line<'a> {
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
        let mut discard_after = self.spans.len() - 1;
        for (i, span) in self.spans.iter_mut().enumerate() {
            let span_len = span.content.len();
            if cur + span_len < start {
                cur += span_len;
                discard_until = i + 1;
                continue;
            }
            if cur >= end {
                discard_after = i;
                break;
            }
            let start = if cur < start { start - cur } else { 0 };
            let end = if cur + span_len > end {
                end - cur
            } else {
                span_len
            };
            match span.content {
                Cow::Borrowed(s) => {
                    span.content = Cow::Borrowed(&s[start..end]);
                }
                Cow::Owned(ref mut s) => {
                    s.drain(end..);
                    s.drain(..start);
                }
            }
            cur += span_len;
        }
        self.spans.drain(discard_after..);
        self.spans.drain(..discard_until);

        self
    }

    fn truncate_start(mut self, start: usize) -> Line<'a> {
        let mut cur = 0;
        let mut discard_until = 0;
        for (i, span) in self.spans.iter_mut().enumerate() {
            let span_len = span.content.len();
            if cur + span_len < start {
                cur += span_len;
                discard_until = i + 1;
                continue;
            }
            let start = if cur < start { start - cur } else { 0 };
            match span.content {
                Cow::Borrowed(s) => {
                    span.content = Cow::Borrowed(&s[start..]);
                }
                Cow::Owned(ref mut s) => {
                    s.drain(..start);
                }
            }
            cur += span_len;
        }
        self.spans.drain(..discard_until);

        self
    }
}
