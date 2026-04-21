use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::ActiveView;

use super::styles::Palette;

/// Render a one-line footer showing either key-binding hints or the active filter bar.
///
/// When `filter_active` is `true` **or** `filter_text` is non-empty, the footer
/// displays the live search query with a block-cursor indicator instead of the
/// normal key hints. This lets the user see the current filter even if they
/// tabbed away from the filter bar temporarily.
///
/// # Arguments
///
/// * `f`           - Ratatui frame.
/// * `area`        - One-line area at the bottom of the screen.
/// * `active_view` - Which panel currently owns focus (selects the hint set).
/// * `palette`     - Active color palette.
/// * `filter_active` - Whether the filter input bar is currently active.
/// * `filter_text`   - Current filter query string.
pub fn render_footer(
    f: &mut Frame,
    area: Rect,
    active_view: &ActiveView,
    palette: &Palette,
    filter_active: bool,
    filter_text: &str,
) {
    // Show the filter bar whenever the filter is active or has text.
    if filter_active || !filter_text.is_empty() {
        render_filter_bar(f, area, palette, filter_text);
        return;
    }

    render_key_hints(f, area, active_view, palette);
}

/// Render the search/filter bar: "/ {text}█".
///
/// The `█` block character acts as a terminal cursor indicator so the user
/// knows the bar is accepting input.
fn render_filter_bar(f: &mut Frame, area: Rect, palette: &Palette, filter_text: &str) {
    let prefix_style = Style::new().fg(palette.label).add_modifier(Modifier::BOLD);
    let text_style = Style::new().fg(palette.foreground);
    let cursor_style = Style::new().fg(palette.label).add_modifier(Modifier::BOLD);

    let spans = vec![
        Span::styled("/ ", prefix_style),
        Span::styled(filter_text, text_style),
        // Block character mimics a cursor so users know where new chars appear.
        Span::styled("\u{2588}", cursor_style),
    ];

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the normal context-sensitive key-binding hints.
fn render_key_hints(f: &mut Frame, area: Rect, active_view: &ActiveView, palette: &Palette) {
    let hints: &[(&str, &str)] = match active_view {
        ActiveView::Tree => &[
            ("q", ": Quit"),
            ("  ↑/↓", ": Navigate"),
            ("  Enter", ": Details"),
            ("  Space", ": Expand"),
            ("  Tab", ": Sort"),
            ("  s", ": Dir"),
            ("  t", ": Agent View"),
            ("  x", ": Kill"),
            ("  c", ": Config"),
            ("  /", ": Filter"),
        ],
        ActiveView::Detail => &[
            ("Esc", ": Back"),
            ("  q", ": Quit"),
            ("  t", ": Agent View"),
            ("  x", ": Kill"),
            ("  c", ": Config"),
        ],
    };

    let key_style = Style::new().fg(palette.label).add_modifier(Modifier::BOLD);
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
