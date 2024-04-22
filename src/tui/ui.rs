use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    widgets::{Paragraph, Widget},
};

pub fn render_title(area: Rect, buf: &mut Buffer, title: &str) {
    Paragraph::new(title).bold().centered().render(area, buf);
}

pub fn render_footer(area: Rect, buf: &mut Buffer, footer: &str) {
    Paragraph::new(footer).centered().render(area, buf);
}
