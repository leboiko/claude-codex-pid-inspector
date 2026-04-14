use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::styles::Palette;

// Red stays hardcoded: "kill" is a destructive action and the color is
// semantic, not thematic. Themes only change neutral chrome.
const WARN_STYLE: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);

/// Render a centered confirmation popup for killing a process.
pub fn render_kill_confirm(f: &mut Frame, pid: u32, process_name: &str, palette: &Palette) {
    let area = centered_rect(50, 7, f.area());

    // Clear the background behind the popup.
    f.render_widget(Clear, area);

    let key_style = Style::new()
        .fg(palette.label)
        .add_modifier(Modifier::BOLD);
    let text_style = Style::new().fg(palette.foreground);
    let dim_style = palette.dim_style();

    let block = Block::default()
        .title(" Kill Process ")
        .title_style(WARN_STYLE)
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Red));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Kill ", text_style),
            Span::styled(process_name, WARN_STYLE),
            Span::styled(" (PID ", text_style),
            Span::styled(pid.to_string(), WARN_STYLE),
            Span::styled(")?", text_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ", dim_style),
            Span::styled("y", key_style),
            Span::styled(" to confirm, ", dim_style),
            Span::styled("n", key_style),
            Span::styled(" or ", dim_style),
            Span::styled("Esc", key_style),
            Span::styled(" to cancel", dim_style),
        ]),
    ];

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Render a centered result popup after a kill attempt.
pub fn render_kill_result(f: &mut Frame, message: &str, palette: &Palette) {
    let area = centered_rect(50, 5, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Result ")
        .title_style(palette.header_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", message),
            Style::new().fg(palette.foreground),
        )),
    ];

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Compute a centered rectangle of `width` columns and `height` rows.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
