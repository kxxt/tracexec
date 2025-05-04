use itertools::chain;
use ratatui::{
  buffer::Buffer,
  layout::{Constraint, Layout, Rect},
  style::Stylize,
  text::{Line, Span},
  widgets::{Block, Paragraph, StatefulWidget, StatefulWidgetRef, Widget, Wrap},
};
use tui_popup::Popup;

use crate::{
  action::ActivePopup, cli::options::ActivePane, pty::PtySize, tui::backtrace_popup::BacktracePopup,
};

use super::{
  super::{
    breakpoint_manager::BreakPointManager,
    copy_popup::CopyPopup,
    details_popup::DetailsPopup,
    error_popup::InfoPopup,
    help::{fancy_help_desc, help, help_item, help_key},
    hit_manager::HitManager,
    theme::THEME,
    ui::render_title,
  },
  App, AppLayout,
};

impl Widget for &mut App {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // Create a space for header, todo list and the footer.
    let vertical = Layout::vertical([
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Min(0),
      Constraint::Length(2),
    ]);
    let [header_area, search_bar_area, rest_area, footer_area] = vertical.areas(area);
    let horizontal_constraints = [
      Constraint::Percentage(self.split_percentage),
      Constraint::Percentage(100 - self.split_percentage),
    ];
    let [event_area, term_area] = (if self.layout == AppLayout::Horizontal {
      Layout::horizontal
    } else {
      Layout::vertical
    })(horizontal_constraints)
    .areas(rest_area);
    let mut title = vec![Span::from(" tracexec "), env!("CARGO_PKG_VERSION").into()];
    if !self.active_experiments.is_empty() {
      title.push(Span::from(" with "));
      title.push(Span::from("experimental ").yellow());
      for (i, &f) in self.active_experiments.iter().enumerate() {
        title.push(Span::from(f).yellow());
        if i != self.active_experiments.len() - 1 {
          title.push(Span::from(", "));
        }
      }
      title.push(Span::from(" feature(s) active"));
    }
    render_title(header_area, buf, Line::from(title));
    if let Some(query_builder) = self.query_builder.as_mut() {
      query_builder.render(search_bar_area, buf);
    }

    self.render_help(footer_area, buf);

    if event_area.width < 4 || (self.term.is_some() && term_area.width < 4) {
      Paragraph::new("Terminal\nor\npane\ntoo\nsmall").render(rest_area, buf);
      return;
    }

    if event_area.height < 4 || (self.term.is_some() && term_area.height < 4) {
      Paragraph::new("Terminal or pane too small").render(rest_area, buf);
      return;
    }

    // resize
    if self.should_handle_internal_resize {
      self.should_handle_internal_resize = false;
      // Set the window size of the event list
      self.event_list.max_window_len = event_area.height as usize - 2;
      self.event_list.set_window((
        self.event_list.get_window().0,
        self.event_list.get_window().0 + self.event_list.max_window_len,
      ));
      if let Some(term) = self.term.as_mut() {
        term
          .resize(PtySize {
            rows: term_area.height - 2,
            cols: term_area.width - 2,
            pixel_width: 0,
            pixel_height: 0,
          })
          .unwrap();
      }
    }

    let block = Block::default()
      .title("Events")
      .borders(ratatui::widgets::Borders::ALL)
      .border_style(if self.active_pane == ActivePane::Events {
        THEME.active_border
      } else {
        THEME.inactive_border
      })
      .title(self.event_list.statistics());
    let inner = block.inner(event_area);
    block.render(event_area, buf);
    self.event_list.render(inner, buf);
    if let Some(term) = self.term.as_mut() {
      let block = Block::default()
        .title("Terminal")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(if self.active_pane == ActivePane::Terminal {
          THEME.active_border
        } else {
          THEME.inactive_border
        });
      term.render(block.inner(term_area), buf);
      block.render(term_area, buf);
    }

    if let Some(breakpoint_mgr_state) = self.breakpoint_manager.as_mut() {
      BreakPointManager.render_ref(rest_area, buf, breakpoint_mgr_state);
    }

    if let Some(h) = self.hit_manager_state.as_mut() {
      if h.visible {
        HitManager.render(rest_area, buf, h);
      }
    }

    // popups
    for popup in self.popup.iter_mut() {
      match popup {
        ActivePopup::Help => {
          let popup = Popup::new(help(rest_area))
            .title("Help")
            .style(THEME.help_popup);
          popup.render(area, buf);
        }
        ActivePopup::CopyTargetSelection(state) => {
          CopyPopup.render_ref(area, buf, state);
        }
        ActivePopup::InfoPopup(state) => {
          InfoPopup.render(area, buf, state);
        }
        ActivePopup::Backtrace(state) => {
          BacktracePopup.render_ref(rest_area, buf, state);
        }
        ActivePopup::ViewDetails(state) => {
          DetailsPopup::new(self.clipboard.is_some()).render_ref(rest_area, buf, state)
        }
      }
    }
  }
}

impl App {
  fn render_help(&self, area: Rect, buf: &mut Buffer) {
    let mut items = Vec::from_iter(
      Some(help_item!("Ctrl+S", "Switch\u{00a0}Pane"))
        .filter(|_| self.term.is_some())
        .into_iter()
        .flatten(),
    );

    if let Some(popup) = &self.popup.last() {
      items.extend(help_item!("Q", "Close\u{00a0}Popup"));
      match popup {
        ActivePopup::ViewDetails(state) => {
          if state.active_tab() == "Info" {
            items.extend(help_item!("W/S", "Move\u{00a0}Focus"));
          }
          items.extend(help_item!("←/Tab/→", "Switch\u{00a0}Tab"));
        }
        ActivePopup::CopyTargetSelection(state) => {
          items.extend(help_item!("Enter", "Choose"));
          items.extend(state.help_items())
        }
        _ => {}
      }
    } else if let Some(breakpoint_manager) = self.breakpoint_manager.as_ref() {
      items.extend(breakpoint_manager.help());
    } else if self.hit_manager_state.as_ref().is_some_and(|x| x.visible) {
      items.extend(self.hit_manager_state.as_ref().unwrap().help());
    } else if let Some(query_builder) = self.query_builder.as_ref().filter(|q| q.editing()) {
      items.extend(query_builder.help());
    } else if self.active_pane == ActivePane::Events {
      items.extend(help_item!("F1", "Help"));
      if self.term.is_some() {
        items.extend(help_item!("G/S", "Grow/Shrink\u{00a0}Pane"));
        items.extend(help_item!("Alt+L", "Layout"));
      }
      items.extend(chain!(
        help_item!(
          "F",
          if self.event_list.is_following() {
            "Unfollow"
          } else {
            "Follow"
          }
        ),
        help_item!(
          "E",
          if self.event_list.is_env_in_cmdline() {
            "Hide\u{00a0}Env"
          } else {
            "Show\u{00a0}Env"
          }
        ),
        help_item!(
          "W",
          if self.event_list.is_cwd_in_cmdline() {
            "Hide\u{00a0}CWD"
          } else {
            "Show\u{00a0}CWD"
          }
        ),
        help_item!("V", "View"),
        help_item!("Ctrl+F", "Search"),
      ));
      if self.event_list.selection_index().is_some() {
        items.extend(help_item!("U", "GoTo Parent"));
        items.extend(help_item!("T", "Backtrace"));
      }
      if let Some(h) = self.hit_manager_state.as_ref() {
        items.extend(help_item!("B", "Breakpoints"));
        if h.count() > 0 {
          items.extend([
            help_key("Z"),
            fancy_help_desc(format!("Hits({})", h.count())),
            "\u{200b}".into(),
          ])
        } else {
          items.extend(help_item!("Z", "Hits"));
        }
      }
      if self.clipboard.is_some() {
        items.extend(help_item!("C", "Copy"));
      }
      if let Some(query_builder) = self.query_builder.as_ref() {
        items.extend(query_builder.help());
      }
      items.extend(help_item!("Q", "Quit"));
    } else {
      // Terminal
      if let Some(h) = self.hit_manager_state.as_ref() {
        if h.count() > 0 {
          items.extend([
            help_key("Ctrl+S,\u{00a0}Z"),
            fancy_help_desc(format!("Hits({})", h.count())),
            "\u{200b}".into(),
          ]);
        }
      }
    };

    let line = Line::default().spans(items);
    Paragraph::new(line)
      .wrap(Wrap { trim: false })
      .centered()
      .render(area, buf);
  }
}
