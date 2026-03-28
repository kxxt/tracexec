use ratatui::{
  buffer::Buffer,
  layout::{
    Constraint,
    Layout,
    Rect,
  },
  style::Stylize,
  text::{
    Line,
    Span,
  },
  widgets::{
    Block,
    Paragraph,
    StatefulWidget,
    StatefulWidgetRef,
    Widget,
    Wrap,
  },
};
use tracexec_core::{
  cli::options::ActivePane,
  pty::PtySize,
};
use tui_popup::Popup;

use super::{
  super::{
    breakpoint_manager::BreakPointManager,
    copy_popup::CopyPopup,
    details_popup::DetailsPopup,
    error_popup::InfoPopup,
    help::{
      fancy_help_desc,
      help,
      help_item,
      help_key,
    },
    hit_manager::HitManager,
    ui::render_title,
  },
  App,
  AppLayout,
};
use crate::{
  action::ActivePopup,
  backtrace_popup::BacktracePopup,
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
    render_title(header_area, buf, Line::from(title), self.theme);
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
        self.theme.active_border
      } else {
        self.theme.inactive_border
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
          self.theme.active_border
        } else {
          self.theme.inactive_border
        });
      term.render(block.inner(term_area), buf);
      block.render(term_area, buf);
    }

    if let Some(breakpoint_mgr_state) = self.breakpoint_manager.as_mut() {
      BreakPointManager.render_ref(rest_area, buf, breakpoint_mgr_state);
    }

    if let Some(h) = self.hit_manager_state.as_mut()
      && h.visible
    {
      HitManager.render(rest_area, buf, h);
    }

    // popups
    for popup in self.popup.iter_mut() {
      match popup {
        ActivePopup::Help => {
          let popup = Popup::new(help(rest_area, &self.key_bindings, self.theme))
            .title("Help")
            .style(self.theme.help_popup);
          Widget::render(popup, area, buf);
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
    /// Compat arrow key pairs into a single item for more concise help display.
    fn compact_pair(a: String, b: String) -> String {
      let is_simple = |s: &str| s.chars().count() == 1;
      let is_alpha = |s: &str| s.chars().all(|ch| ch.is_ascii_alphabetic());
      if is_simple(&a) && is_simple(&b) && !is_alpha(&a) && !is_alpha(&b) {
        format!("{a}{b}")
      } else {
        format!("{a}/{b}")
      }
    }

    let mut items = Vec::from_iter(
      Some(help_item!(
        self.key_bindings.switch_pane.display(),
        "Switch\u{00a0}Pane",
        self.theme
      ))
      .filter(|_| self.term.is_some())
      .into_iter()
      .flatten(),
    );

    if let Some(popup) = &self.popup.last() {
      items.extend(help_item!(
        self.key_bindings.close_popup.display(),
        "Close\u{00a0}Popup",
        self.theme
      ));
      match popup {
        ActivePopup::ViewDetails(state) => {
          state.update_help(&self.key_bindings, &mut items);
        }
        ActivePopup::CopyTargetSelection(state) => {
          items.extend(help_item!(
            self.key_bindings.copy_choose.display(),
            "Choose",
            self.theme
          ));
          items.extend(state.help_items())
        }
        ActivePopup::Backtrace(state) => {
          state.list.update_help(&self.key_bindings, &mut items);
        }
        _ => {}
      }
    } else if let Some(breakpoint_manager) = self.breakpoint_manager.as_ref() {
      items.extend(breakpoint_manager.help(&self.key_bindings, self.theme));
    } else if self.hit_manager_state.as_ref().is_some_and(|x| x.visible) {
      items.extend(
        self
          .hit_manager_state
          .as_ref()
          .unwrap()
          .help(&self.key_bindings),
      );
    } else if let Some(query_builder) = self.query_builder.as_ref().filter(|q| q.editing()) {
      items.extend(query_builder.help(&self.key_bindings, self.theme));
    } else if self.active_pane == ActivePane::Events {
      items.extend(help_item!(
        self.key_bindings.help.display(),
        "Help",
        self.theme
      ));
      self.event_list.update_help(&self.key_bindings, &mut items);
      if self.term.is_some() {
        items.extend(help_item!(
          format!(
            "{}/{}",
            self.key_bindings.event_grow_pane.display(),
            self.key_bindings.event_shrink_pane.display()
          ),
          "Grow/Shrink\u{00a0}Pane",
          self.theme
        ));
        items.extend(help_item!(
          self.key_bindings.switch_layout.display(),
          "Layout",
          self.theme
        ));
      }
      if let Some(h) = self.hit_manager_state.as_ref() {
        items.extend(help_item!(
          self.key_bindings.event_breakpoints.display(),
          "Breakpoints",
          self.theme
        ));
        if h.count() > 0 {
          items.extend([
            help_key(self.key_bindings.event_hits.display(), self.theme),
            fancy_help_desc(format!("Hits({})", h.count()), self.theme),
            "\u{200b}".into(),
          ])
        } else {
          items.extend(help_item!(
            self.key_bindings.event_hits.display(),
            "Hits",
            self.theme
          ));
        }
      }
      if let Some(query_builder) = self.query_builder.as_ref() {
        items.extend(query_builder.help(&self.key_bindings, self.theme));
      }
      items.extend(help_item!(
        self.key_bindings.quit.display(),
        "Quit",
        self.theme
      ));
    } else {
      // Terminal
      if let Some(term) = self.term.as_ref() {
        if term.is_scrollback_mode() {
          // In scrollback mode - show navigation keys highlighted
          items.extend([
            help_key(
              self.key_bindings.terminal_toggle_scrollback.display(),
              self.theme,
            ),
            fancy_help_desc("Exit\u{00a0}Scroll", self.theme),
            "\u{200b}".into(),
          ]);
          items.extend(help_item!(
            compact_pair(
              self.key_bindings.terminal_scroll_up.display(),
              self.key_bindings.terminal_scroll_down.display()
            ),
            "Scroll",
            self.theme
          ));
          items.extend(help_item!(
            format!(
              "{}/{}",
              self.key_bindings.terminal_page_up.display(),
              self.key_bindings.terminal_page_down.display()
            ),
            "Page",
            self.theme
          ));
          items.extend(help_item!(
            format!(
              "{}/{}",
              self.key_bindings.terminal_scroll_top.display(),
              self.key_bindings.terminal_scroll_bottom.display()
            ),
            "Jump",
            self.theme
          ));
        } else {
          // Normal mode - show how to enter scrollback
          items.extend(help_item!(
            self.key_bindings.terminal_toggle_scrollback.display(),
            "Scroll",
            self.theme
          ));
        }
      }
      if let Some(h) = self.hit_manager_state.as_ref()
        && h.count() > 0
      {
        items.extend([
          help_key(
            format!(
              "{},\u{00a0}{}",
              self.key_bindings.switch_pane.display(),
              self.key_bindings.event_hits.display()
            ),
            self.theme,
          ),
          fancy_help_desc(format!("Hits({})", h.count()), self.theme),
          "\u{200b}".into(),
        ]);
      }
    };

    let line = Line::default().spans(items);
    Paragraph::new(line)
      .wrap(Wrap { trim: false })
      .centered()
      .render(area, buf);
  }
}
