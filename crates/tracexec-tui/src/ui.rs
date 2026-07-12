use ratatui::{
  buffer::Buffer,
  layout::Rect,
  text::Text,
  widgets::{
    Paragraph,
    Widget,
  },
};
use tui_widget_list::{
  ListBuilder,
  ListState,
};

use super::theme::Theme;

pub fn render_title<'a>(area: Rect, buf: &mut Buffer, title: impl Into<Text<'a>>, theme: &Theme) {
  Paragraph::new(title)
    .style(theme.app_title)
    .render(area, buf);
}

pub fn paragraph_list_builder<'a, T>(
  items: &'a [T],
  theme: &'a Theme,
  render: impl Fn(&T, bool, &Theme) -> Paragraph<'static> + 'a,
) -> ListBuilder<'a, Paragraph<'static>> {
  ListBuilder::new(move |context| {
    let paragraph = render(&items[context.index], context.is_selected, theme);
    let line_count = paragraph
      .line_count(context.cross_axis_size)
      .try_into()
      .unwrap_or(u16::MAX);
    (paragraph, line_count)
  })
}

pub fn select_first_if_unset(state: &mut ListState, item_count: usize) {
  if item_count > 0 && state.selected.is_none() {
    state.select(Some(0));
  }
}

#[cfg(test)]
mod tests {
  use insta::assert_snapshot;
  use ratatui::{
    Terminal,
    backend::TestBackend,
  };
  use tui_widget_list::ListState;

  use super::{
    render_title,
    select_first_if_unset,
  };
  use crate::theme::current_theme;

  #[test]
  fn snapshot_render_title() {
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).unwrap();
    terminal
      .draw(|frame| {
        render_title(
          frame.area(),
          frame.buffer_mut(),
          "tracexec",
          current_theme(),
        );
      })
      .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert_snapshot!(rendered);
  }

  #[test]
  fn select_first_only_when_items_exist_and_selection_is_unset() {
    let mut state = ListState::default();

    select_first_if_unset(&mut state, 0);
    assert_eq!(state.selected, None);

    select_first_if_unset(&mut state, 2);
    assert_eq!(state.selected, Some(0));

    state.select(Some(1));
    select_first_if_unset(&mut state, 2);
    assert_eq!(state.selected, Some(1));
  }
}
