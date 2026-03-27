use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::ActiveView;

/// Style for key bindings (e.g. "q", "Enter").
const KEY_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);

/// Style for the description text following each key (e.g. ": Quit").
const DESC_STYLE: Style = Style::new().fg(Color::DarkGray);

/// Render a one-line footer showing context-sensitive key binding hints.
///
/// # Arguments
///
/// * `f`           - Ratatui frame to render into.
/// * `area`        - Rect defining where the footer is placed.
/// * `active_view` - Which view is currently active; controls which hints are shown.
pub fn render_footer(f: &mut Frame, area: Rect, active_view: &ActiveView) {
    let hints: &[(&str, &str)] = match active_view {
        ActiveView::Tree => &[
            ("q", ": Quit"),
            ("  ↑/↓", ": Navigate"),
            ("  Enter", ": Details"),
            ("  Space", ": Expand/Collapse"),
            ("  r", ": Refresh"),
        ],
        ActiveView::Detail => &[("Esc", ": Back"), ("  q", ": Quit")],
    };

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            [
                Span::styled(*key, KEY_STYLE),
                Span::styled(*desc, DESC_STYLE),
            ]
        })
        .collect();

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
