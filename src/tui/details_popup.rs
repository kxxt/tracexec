use std::{rc::Rc, sync::Arc};

use itertools::Itertools;
use nix::errno::Errno;
use ratatui::{
  buffer::Buffer,
  layout::{Rect, Size},
  style::{Color, Stylize},
  text::{Line, Span},
  widgets::{Paragraph, Widget, WidgetRef, Wrap},
};
use tui_popup::SizedWidgetRef;

use crate::{event::TracerEvent, proc::BaselineInfo};

#[derive(Debug, Clone)]
pub struct DetailsPopup {
  event: Arc<TracerEvent>,
  size: Size,
  baseline: Rc<BaselineInfo>,
  details: Vec<(&'static str, Line<'static>)>,
}

impl DetailsPopup {
  pub fn new(event: Arc<TracerEvent>, size: Size, baseline: Rc<BaselineInfo>) -> Self {
    let mut details = vec![(
      if matches!(event.as_ref(), TracerEvent::Exec(_)) {
        " Cmdline "
      } else {
        " Details "
      },
      event.to_tui_line(&baseline, true),
    )];
    let event_cloned = event.clone();
    if let TracerEvent::Exec(exec) = event_cloned.as_ref() {
      details.extend([
        (" Pid ", Line::from(exec.pid.to_string())),
        (" Result ", {
          if exec.result == 0 {
            "0 (Success)".green().into()
          } else {
            format!("{} ({})", exec.result, Errno::from_raw(-exec.result as i32))
              .red()
              .into()
          }
        }),
        (
          " Cwd ",
          Span::from(exec.cwd.to_string_lossy().to_string()).into(),
        ),
        (" Comm ", exec.comm.to_string().into()),
        (
          " Filename ",
          Span::from(exec.filename.to_string_lossy().to_string()).into(),
        ),
        (" Argv ", TracerEvent::argv_to_string(&exec.argv).into()),
        (
          " Interpreters ",
          TracerEvent::interpreters_to_string(&exec.interpreter).into(),
        ),
      ]);
    };
    Self {
      event,
      size: Size {
        width: size.width,
        height: size.height - 2,
      },
      baseline,
      details,
    }
  }

  fn label<'a>(content: &'a str) -> Line<'a> {
    content.bold().fg(Color::Black).bg(Color::LightGreen).into()
  }
}

impl WidgetRef for DetailsPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer) {
    let text = self
      .details
      .iter()
      .flat_map(|(label, line)| [Self::label(label), line.clone()])
      .collect_vec();

    Paragraph::new(text)
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
