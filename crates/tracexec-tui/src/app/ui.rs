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
      HelpItem,
      fancy_help_desc,
      help,
      help_item,
      help_key,
    },
    hit_manager::HitManager,
    mouse::{
      HelpBarEntry,
      position_in_rect,
    },
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

    // Store layout areas for mouse hit testing
    self.layout_areas.event_list_inner = inner;
    self.layout_areas.event_list_outer = event_area;
    self.layout_areas.footer = footer_area;
    self.layout_areas.rest_area = rest_area;

    if let Some(term) = self.term.as_mut() {
      let block = Block::default()
        .title("Terminal")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(if self.active_pane == ActivePane::Terminal {
          self.theme.active_border
        } else {
          self.theme.inactive_border
        });
      let term_inner = block.inner(term_area);
      term.render(term_inner, buf);
      block.render(term_area, buf);
      self.layout_areas.terminal_inner = Some(term_inner);
      self.layout_areas.terminal_outer = Some(term_area);
    } else {
      self.layout_areas.terminal_inner = None;
      self.layout_areas.terminal_outer = None;
    }

    // Clear title bar entries before overlay rendering
    self.layout_areas.title_bar_entries.clear();
    let mut title_bar_items: Vec<(Rect, HelpItem<'static>)> = Vec::new();

    if let Some(breakpoint_mgr_state) = self.breakpoint_manager.as_mut() {
      BreakPointManager.render_ref(rest_area, buf, breakpoint_mgr_state);
      title_bar_items.append(&mut breakpoint_mgr_state.title_bar_items);
    }

    if let Some(h) = self.hit_manager_state.as_mut()
      && h.visible
    {
      HitManager.render(rest_area, buf, h);
      title_bar_items.append(&mut h.title_bar_items);
    }

    // Render title bar items with hover support
    for (item_area, item) in &title_bar_items {
      let is_hovered = item.key_event.is_some()
        && position_in_rect(self.hover_state.col, self.hover_state.row, item_area);
      let key_w = item.key_span.width() as u16;
      if is_hovered {
        let key_span = Span::styled(item.key_span.content.clone(), self.theme.help_key_hover);
        let desc_span = Span::styled(item.desc_span.content.clone(), self.theme.help_desc_hover);
        buf.set_span(item_area.x, item_area.y, &key_span, key_w);
        buf.set_span(
          item_area.x + key_w,
          item_area.y,
          &desc_span,
          item_area.width.saturating_sub(key_w),
        );
      } else {
        buf.set_span(item_area.x, item_area.y, &item.key_span, key_w);
        buf.set_span(
          item_area.x + key_w,
          item_area.y,
          &item.desc_span,
          item_area.width.saturating_sub(key_w),
        );
      }
      if let Some(ke) = item.key_event {
        self.layout_areas.title_bar_entries.push(HelpBarEntry {
          area: *item_area,
          key_event: ke,
        });
      }
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
  fn render_help(&mut self, area: Rect, buf: &mut Buffer) {
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

    let mut items: Vec<HelpItem<'_>> = Vec::new();

    if self.term.is_some() {
      items.push(help_item!(
        self.key_bindings.switch_pane.display(),
        "Switch\u{00a0}Pane",
        self.theme,
        &self.key_bindings.switch_pane
      ));
    }

    if let Some(popup) = &self.popup.last() {
      items.push(help_item!(
        self.key_bindings.close_popup.display(),
        "Close\u{00a0}Popup",
        self.theme,
        &self.key_bindings.close_popup
      ));
      match popup {
        ActivePopup::ViewDetails(state) => {
          state.update_help(&self.key_bindings, &mut items);
        }
        ActivePopup::CopyTargetSelection(state) => {
          items.push(help_item!(
            self.key_bindings.copy_choose.display(),
            "Choose",
            self.theme,
            &self.key_bindings.copy_choose
          ));
          items.extend(state.help_items());
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
      items.push(help_item!(
        self.key_bindings.help.display(),
        "Help",
        self.theme,
        &self.key_bindings.help
      ));
      self.event_list.update_help(&self.key_bindings, &mut items);
      if self.term.is_some() {
        items.push(help_item!(
          format!(
            "{}/{}",
            self.key_bindings.event_grow_pane.display(),
            self.key_bindings.event_shrink_pane.display()
          ),
          "Grow/Shrink\u{00a0}Pane",
          self.theme
        ));
        items.push(help_item!(
          self.key_bindings.switch_layout.display(),
          "Layout",
          self.theme,
          &self.key_bindings.switch_layout
        ));
      }
      if let Some(h) = self.hit_manager_state.as_ref() {
        items.push(help_item!(
          self.key_bindings.event_breakpoints.display(),
          "Breakpoints",
          self.theme,
          &self.key_bindings.event_breakpoints
        ));
        if h.count() > 0 {
          items.push(HelpItem {
            key_span: help_key(self.key_bindings.event_hits.display(), self.theme),
            desc_span: fancy_help_desc(format!("Hits({})", h.count()), self.theme),
            key_event: self.key_bindings.event_hits.first_key_event(),
          });
        } else {
          items.push(help_item!(
            self.key_bindings.event_hits.display(),
            "Hits",
            self.theme,
            &self.key_bindings.event_hits
          ));
        }
      }
      if let Some(query_builder) = self.query_builder.as_ref() {
        items.extend(query_builder.help(&self.key_bindings, self.theme));
      }
      items.push(help_item!(
        self.key_bindings.quit.display(),
        "Quit",
        self.theme,
        &self.key_bindings.quit
      ));
    } else {
      // Terminal
      if let Some(term) = self.term.as_ref() {
        if term.is_scrollback_mode() {
          // In scrollback mode - show navigation keys highlighted
          items.push(HelpItem {
            key_span: help_key(
              self.key_bindings.terminal_toggle_scrollback.display(),
              self.theme,
            ),
            desc_span: fancy_help_desc("Exit\u{00a0}Scroll", self.theme),
            key_event: self
              .key_bindings
              .terminal_toggle_scrollback
              .first_key_event(),
          });
          items.push(help_item!(
            compact_pair(
              self.key_bindings.terminal_scroll_up.display(),
              self.key_bindings.terminal_scroll_down.display()
            ),
            "Scroll",
            self.theme
          ));
          items.push(help_item!(
            format!(
              "{}/{}",
              self.key_bindings.terminal_page_up.display(),
              self.key_bindings.terminal_page_down.display()
            ),
            "Page",
            self.theme
          ));
          items.push(help_item!(
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
          items.push(help_item!(
            self.key_bindings.terminal_toggle_scrollback.display(),
            "Scroll",
            self.theme,
            &self.key_bindings.terminal_toggle_scrollback
          ));
        }
      }
      if let Some(h) = self.hit_manager_state.as_ref()
        && h.count() > 0
      {
        items.push(HelpItem {
          key_span: help_key(
            format!(
              "{},\u{00a0}{}",
              self.key_bindings.switch_pane.display(),
              self.key_bindings.event_hits.display()
            ),
            self.theme,
          ),
          desc_span: fancy_help_desc(format!("Hits({})", h.count()), self.theme),
          key_event: None, // compound action, not directly clickable
        });
      }
    };

    // Render help items with position tracking for mouse click support.
    // We simulate wrapping and centering manually instead of using Paragraph
    // so we can record the screen position of each clickable item.
    let mut help_entries = Vec::new();

    // First, compute wrapped lines: each line holds (HelpItem, width)
    let mut lines: Vec<Vec<(usize, u16)>> = vec![vec![]]; // (index, width)
    let mut current_line_width: u16 = 0;

    for (idx, item) in items.iter().enumerate() {
      let w = item.width();
      if current_line_width > 0 && current_line_width + w > area.width {
        lines.push(vec![]);
        current_line_width = 0;
      }
      lines.last_mut().unwrap().push((idx, w));
      current_line_width += w;
    }

    // Render each line, centered
    for (line_idx, line) in lines.iter().enumerate() {
      let y = area.y + line_idx as u16;
      if y >= area.y + area.height {
        break;
      }
      let line_width: u16 = line.iter().map(|(_, w)| *w).sum();
      let offset = area.width.saturating_sub(line_width) / 2;
      let mut x = area.x + offset;

      for &(idx, width) in line {
        let item = &items[idx];
        let item_rect = Rect {
          x,
          y,
          width,
          height: 1,
        };
        let is_clickable = item.key_event.is_some();
        let is_hovered =
          is_clickable && position_in_rect(self.hover_state.col, self.hover_state.row, &item_rect);
        // Record clickable region
        if let Some(ke) = item.key_event {
          help_entries.push(HelpBarEntry {
            area: item_rect,
            key_event: ke,
          });
        }
        // Render spans with hover styling when applicable
        let key_w = item.key_span.width() as u16;
        if is_hovered {
          // Apply hover style directly without re-wrapping (content already has padding)
          let key_span = Span::styled(item.key_span.content.clone(), self.theme.help_key_hover);
          let desc_span = Span::styled(item.desc_span.content.clone(), self.theme.help_desc_hover);
          buf.set_span(x, y, &key_span, key_w);
          buf.set_span(x + key_w, y, &desc_span, width - key_w);
        } else {
          buf.set_span(x, y, &item.key_span, key_w);
          buf.set_span(x + key_w, y, &item.desc_span, width - key_w);
        }
        x += width;
      }
    }

    self.help_bar_entries = help_entries;
  }
}
