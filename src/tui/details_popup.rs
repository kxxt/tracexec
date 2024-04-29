use std::{rc::Rc, sync::Arc};

use ratatui::{
  buffer::Buffer,
  layout::{Rect, Size},
  widgets::{Paragraph, Widget, WidgetRef, Wrap},
};
use tui_popup::SizedWidgetRef;

use crate::{event::TracerEvent, proc::BaselineInfo};

#[derive(Debug, Clone)]
pub struct DetailsPopup {
  event: Arc<TracerEvent>,
  size: Size,
  baseline: Rc<BaselineInfo>,
}

impl DetailsPopup {
  pub fn new(event: Arc<TracerEvent>, size: Size, baseline: Rc<BaselineInfo>) -> Self {
    Self {
      event,
      size: Size {
        width: size.width,
        height: size.height - 2,
      },
      baseline,
    }
  }
}

impl WidgetRef for DetailsPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer) {
    Paragraph::new(self.event.to_tui_line(&self.baseline))
      .wrap(Wrap { trim: false })
      .render(area, buf);
  }
}

impl SizedWidgetRef for DetailsPopup {
  fn width(&self) -> usize {
    self.size.width as usize
  }

  fn height(&self) -> usize {
    self.size.height as usize
  }
}
