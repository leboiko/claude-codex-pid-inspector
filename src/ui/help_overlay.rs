//! Help overlay modal rendered on top of whatever view is active.
//!
//! # Binding registry
//!
//! Bindings are stored as a `const` table in this module — a single slice of
//! `(category, key, description)` tuples. The renderer groups them by category
//! header. Adding or renaming a binding requires only a change to
//! [`HELP_ENTRIES`]; the rendering function stays unchanged.
//!
//! # Design decision
//!
//! A static table was chosen over a dynamic registry that mirrors
//! `map_key_to_action` because it keeps the code simpler: the overlay is
//! informational and needs to reflect the spec, not auto-detect remaps.

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::styles::Palette;

/// One row in the help table: `(category, key, description)`.
type HelpEntry = (&'static str, &'static str, &'static str);

/// Authoritative list of keybindings shown in the help overlay.
///
/// Grouped by category. The renderer draws a section header whenever the
/// category changes from one entry to the next.
const HELP_ENTRIES: &[HelpEntry] = &[
    // Navigation
    ("Navigation", "↑ / k", "move up"),
    ("Navigation", "↓ / j", "move down"),
    ("Navigation", "Enter", "open detail"),
    ("Navigation", "Esc", "back to tree"),
    // View
    ("View", "T", "agent view"),
    ("View", "g", "project grouping"),
    ("View", "?", "this help"),
    // Filter
    ("Filter", "/", "search"),
    ("Filter", "F", "cycle curated filter"),
    ("Filter", "z", "clear filter"),
    // Sort
    ("Sort", "Tab / Shift+Tab", "cycle column"),
    ("Sort", "s", "toggle sort direction"),
    // Action
    ("Action", "Space", "expand / collapse"),
    ("Action", "x", "kill process"),
    ("Action", "c", "config"),
    // System
    ("System", "q / Ctrl+C", "quit"),
];

/// Render a centered modal help overlay.
///
/// The popup occupies 70% of the terminal width and 80% of the terminal height.
/// The underlying content is left visible; [`ratatui::widgets::Clear`] erases
/// only the popup's own rectangle so the background shows through the rest.
///
/// Dismiss: `Esc` closes. Any other bound key closes **and** executes the
/// action (handled in `App::map_key_to_action`).
///
/// # Arguments
///
/// * `f`       - Ratatui frame.
/// * `palette` - Active color palette.
pub fn render_help_overlay(f: &mut Frame, palette: &Palette) {
    let area = centered_pct(70, 80, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Help — press Esc to close ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style())
        .style(Style::new().bg(palette.background));

    let lines = build_help_lines(palette);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Build one [`Line`] per entry, inserting a section header whenever the
/// category changes.
fn build_help_lines(palette: &Palette) -> Vec<Line<'static>> {
    let section_style = Style::new()
        .fg(palette.header)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::UNDERLINED);
    let key_style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
    let desc_style = Style::new().fg(palette.foreground);
    let sep_style = Style::new().fg(palette.dim);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(HELP_ENTRIES.len() + 12);
    lines.push(Line::from(""));

    let mut current_category = "";
    for &(cat, key, desc) in HELP_ENTRIES {
        if cat != current_category {
            if !current_category.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(cat, section_style),
            ]));
            current_category = cat;
        }
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(key, key_style),
            Span::styled("  —  ", sep_style),
            Span::styled(desc, desc_style),
        ]));
    }

    lines.push(Line::from(""));
    lines
}

/// Compute a rectangle centered in `area` at `pct_w`% width and `pct_h`% height.
fn centered_pct(pct_w: u16, pct_h: u16, area: Rect) -> Rect {
    let width = (area.width * pct_w / 100).max(20);
    let height = (area.height * pct_h / 100).max(10);

    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
