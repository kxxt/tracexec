use std::{
  ops::{Deref, DerefMut},
  sync::Arc,
};

use itertools::Itertools;
use nix::errno::Errno;
use ratatui::{
  buffer::Buffer,
  layout::{Alignment::Center, Rect, Size},
  style::{Color, Stylize},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, StatefulWidgetRef, Widget, Wrap},
};
use tui_scrollview::{ScrollView, ScrollViewState};

use crate::{event::TracerEvent, proc::BaselineInfo};

use super::help::{help_desc, help_key};

pub struct DetailsPopup {
  enable_copy: bool,
}

impl DetailsPopup {
  pub fn new(enable_copy: bool) -> Self {
    Self { enable_copy }
  }
}

#[derive(Debug, Clone)]
pub struct DetailsPopupState {
  event: Arc<TracerEvent>,
  baseline: Arc<BaselineInfo>,
  details: Vec<(&'static str, Line<'static>)>,
  active_index: usize,
  scroll: ScrollViewState,
}

impl DetailsPopupState {
  pub fn new(event: Arc<TracerEvent>, baseline: Arc<BaselineInfo>) -> Self {
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
      baseline,
      details,
      active_index: 0,
      scroll: Default::default(),
    }
  }

  pub fn next(&mut self) {
    self.active_index = (self.active_index + 1).min(self.details.len() - 1);
  }

  pub fn prev(&mut self) {
    self.active_index = self.active_index.saturating_sub(1);
  }

  pub fn selected(&self) -> String {
    self.details[self.active_index].1.to_string()
  }
}

impl Deref for DetailsPopupState {
  type Target = ScrollViewState;

  fn deref(&self) -> &Self::Target {
    &self.scroll
  }
}

impl DerefMut for DetailsPopupState {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.scroll
  }
}

impl StatefulWidgetRef for DetailsPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut DetailsPopupState) {
    let text = state
      .details
      .iter()
      .enumerate()
      .flat_map(|(idx, (label, line))| [self.label(label, idx == state.active_index), line.clone()])
      .collect_vec();

    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let size = Size {
      width: area.width - 1,
      height: paragraph
        .line_count(area.width - 1)
        .try_into()
        .unwrap_or(u16::MAX),
    };

    let block = Block::new()
      .title(" Details ")
      .borders(Borders::TOP | Borders::BOTTOM)
      .title_alignment(Center);
    let inner = block.inner(area);

    let mut scrollview = ScrollView::new(size);
    scrollview.render_widget(
      paragraph,
      Rect {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
      },
    );
    Clear.render(area, buf);
    block.render(area, buf);
    scrollview.render(inner, buf, &mut state.scroll);
  }

  type State = DetailsPopupState;
}

impl DetailsPopup {
  fn label<'a>(&self, content: &'a str, active: bool) -> Line<'a> {
    if !active {
      content.bold().fg(Color::Black).bg(Color::LightGreen).into()
    } else {
      let mut spans = vec![
        content.bold().fg(Color::White).bg(Color::LightMagenta),
        " ".into(),
        "<- ".bold().fg(Color::LightGreen),
      ];
      if self.enable_copy {
        spans.extend([help_key("C"), help_desc("Copy")]);
      }
      spans.into()
    }
  }
}
