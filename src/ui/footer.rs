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

/// Style for warning/confirmation prompts.
const WARN_STYLE: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);

/// Render a one-line footer showing context-sensitive key binding hints.
///
/// If `confirm_kill` contains a PID, shows a kill confirmation prompt.
/// If `kill_result` is set, shows the result message instead.
pub fn render_footer(
    f: &mut Frame,
    area: Rect,
    active_view: &ActiveView,
    confirm_kill: Option<u32>,
    kill_result: Option<&str>,
) {
    if let Some(pid) = confirm_kill {
        let line = Line::from(vec![
            Span::styled("Kill PID ", WARN_STYLE),
            Span::styled(pid.to_string(), WARN_STYLE),
            Span::styled("? ", WARN_STYLE),
            Span::styled("y", KEY_STYLE),
            Span::styled(": Yes  ", DESC_STYLE),
            Span::styled("n/Esc", KEY_STYLE),
            Span::styled(": Cancel", DESC_STYLE),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    if let Some(msg) = kill_result {
        let line = Line::from(Span::styled(msg, WARN_STYLE));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let hints: &[(&str, &str)] = match active_view {
        ActiveView::Tree => &[
            ("q", ": Quit"),
            ("  ↑/↓", ": Navigate"),
            ("  Enter", ": Details"),
            ("  Space", ": Expand"),
            ("  Tab", ": Sort"),
            ("  s", ": Dir"),
            ("  x", ": Kill"),
        ],
        ActiveView::Detail => &[
            ("Esc", ": Back"),
            ("  q", ": Quit"),
            ("  x", ": Kill"),
        ],
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
