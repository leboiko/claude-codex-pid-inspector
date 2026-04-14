use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::ActiveView;

use super::styles::Palette;

/// Render a one-line footer showing context-sensitive key binding hints.
pub fn render_footer(f: &mut Frame, area: Rect, active_view: &ActiveView, palette: &Palette) {
    let hints: &[(&str, &str)] = match active_view {
        ActiveView::Tree => &[
            ("q", ": Quit"),
            ("  ↑/↓", ": Navigate"),
            ("  Enter", ": Details"),
            ("  Space", ": Expand"),
            ("  Tab", ": Sort"),
            ("  s", ": Dir"),
            ("  x", ": Kill"),
            ("  c", ": Config"),
        ],
        ActiveView::Detail => &[
            ("Esc", ": Back"),
            ("  q", ": Quit"),
            ("  x", ": Kill"),
            ("  c", ": Config"),
        ],
    };

    let key_style = Style::new()
        .fg(palette.label)
        .add_modifier(Modifier::BOLD);
    let desc_style = palette.dim_style();

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            [
                Span::styled(*key, key_style),
                Span::styled(*desc, desc_style),
            ]
        })
        .collect();

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
