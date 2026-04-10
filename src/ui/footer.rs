use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::ActiveView;

use super::styles::Palette;

/// Style for the description text following each key (e.g. ": Quit").
const DESC_STYLE: Style = Style::new().fg(Color::DarkGray);

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

    // Theme-aware key color; keep the trailing description dim so the keys pop.
    let key_style = Style::new()
        .fg(palette.label)
        .add_modifier(Modifier::BOLD);

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            [
                Span::styled(*key, key_style),
                Span::styled(*desc, DESC_STYLE),
            ]
        })
        .collect();

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
