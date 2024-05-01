use std::{
  ops::{Deref, DerefMut},
  sync::Arc,
};

use itertools::Itertools;
use nix::errno::Errno;
use ratatui::{
  buffer::Buffer,
  layout::{Alignment::Center, Rect, Size},
  style::{Color, Style, Stylize},
  text::{Line, Span},
  widgets::{
    Block, Borders, Clear, Paragraph, StatefulWidget, StatefulWidgetRef, Tabs, Widget, WidgetRef,
    Wrap,
  },
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
  env: Option<Vec<Line<'static>>>,
  available_tabs: Vec<&'static str>,
  tab_index: usize,
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
    let (env, available_tabs) = if let TracerEvent::Exec(exec) = event_cloned.as_ref() {
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
      let mut env = exec
        .env_diff
        .added
        .iter()
        .map(|(key, value)| {
          let spans = vec![
            "+".fg(Color::LightGreen),
            key.to_string().bold().light_green(),
            "=".yellow().bold(),
            value.to_string().light_green(),
          ];
          Line::default().spans(spans)
        })
        .collect_vec();
      env.extend(
        exec
          .env_diff
          .removed
          .iter()
          .map(|key| {
            let value = baseline.env.get(key).unwrap();
            let spans = vec![
              "-".fg(Color::LightRed),
              key.to_string().bold().light_red(),
              "=".yellow().bold(),
              value.to_string().light_red(),
            ];
            Line::default().spans(spans)
          })
          .collect_vec(),
      );
      env.extend(
        exec
          .env_diff
          .modified
          .iter()
          .flat_map(|(key, new)| {
            let old = baseline.env.get(key).unwrap();
            let spans_old = vec![
              "-".fg(Color::LightRed),
              key.to_string().light_red(),
              "=".yellow().bold(),
              old.to_string().light_red(),
            ];
            let spans_new = vec![
              "+".fg(Color::LightGreen),
              key.to_string().bold().light_green(),
              "=".yellow().bold(),
              new.to_string().light_green(),
            ];
            vec![
              Line::default().spans(spans_old),
              Line::default().spans(spans_new),
            ]
          })
          .collect_vec(),
      );
      env.extend(
        // Unchanged env
        baseline
          .env
          .iter()
          .filter(|(key, _)| !exec.env_diff.is_modified_or_removed(key))
          .map(|(key, value)| {
            let spans = vec![
              " ".into(),
              key.to_string().bold().white(),
              "=".yellow(),
              value.to_string().white(),
            ];
            Line::default().spans(spans)
          }),
      );
      (Some(env), vec!["Info", "Environment"])
    } else {
      (None, vec!["Info"])
    };
    Self {
      event,
      baseline,
      details,
      active_index: 0,
      scroll: Default::default(),
      env,
      available_tabs,
      tab_index: 0,
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

  pub fn next_tab(&mut self) {
    self.tab_index = (self.tab_index + 1).min(self.available_tabs.len() - 1);
  }

  pub fn prev_tab(&mut self) {
    self.tab_index = self.tab_index.saturating_sub(1);
  }

  pub fn circle_tab(&mut self) {
    self.tab_index = (self.tab_index + 1) % self.available_tabs.len();
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
    Clear.render(area, buf);
    let block = Block::new()
      .title(" Details ")
      .borders(Borders::TOP | Borders::BOTTOM)
      .title_alignment(Center);
    let inner = block.inner(area);
    block.render(area, buf);

    // Tabs
    let tabs = Tabs::new(state.available_tabs.clone())
      .highlight_style(Style::default().on_magenta().white())
      .select(state.tab_index);
    // FIXME: Ratatui's tab does not support alignment
    let screen = buf.area;
    let tabs_width = state
      .available_tabs
      .iter()
      .map(|s| s.len() as u16)
      .sum::<u16>()
      + 2 * state.available_tabs.len() as u16;
    let start = screen.right().saturating_sub(tabs_width);
    tabs.render_ref(Rect::new(start, 0, tabs_width, 1), buf);

    // Tab Info
    let paragraph = if state.tab_index == 0 {
      self.info_paragraph(state)
    } else {
      self.env_paragraph(state)
    };

    let size = Size {
      width: area.width - 1,
      height: paragraph
        .line_count(area.width - 1)
        .try_into()
        .unwrap_or(u16::MAX),
    };
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

  fn info_paragraph(&self, state: &DetailsPopupState) -> Paragraph {
    let text = state
      .details
      .iter()
      .enumerate()
      .flat_map(|(idx, (label, line))| [self.label(label, idx == state.active_index), line.clone()])
      .collect_vec();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }

  fn env_paragraph(&self, state: &DetailsPopupState) -> Paragraph {
    let text = state.env.clone().unwrap();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }
}
