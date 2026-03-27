use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const WARN_STYLE: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
const KEY_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
const TEXT_STYLE: Style = Style::new().fg(Color::White);
const DIM_STYLE: Style = Style::new().fg(Color::DarkGray);

/// Render a centered confirmation popup for killing a process.
pub fn render_kill_confirm(f: &mut Frame, pid: u32, process_name: &str) {
    let area = centered_rect(50, 7, f.area());

    // Clear the background behind the popup.
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Kill Process ")
        .title_style(WARN_STYLE)
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Red));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Kill ", TEXT_STYLE),
            Span::styled(process_name, WARN_STYLE),
            Span::styled(" (PID ", TEXT_STYLE),
            Span::styled(pid.to_string(), WARN_STYLE),
            Span::styled(")?", TEXT_STYLE),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ", DIM_STYLE),
            Span::styled("y", KEY_STYLE),
            Span::styled(" to confirm, ", DIM_STYLE),
            Span::styled("n", KEY_STYLE),
            Span::styled(" or ", DIM_STYLE),
            Span::styled("Esc", KEY_STYLE),
            Span::styled(" to cancel", DIM_STYLE),
        ]),
    ];

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Render a centered result popup after a kill attempt.
pub fn render_kill_result(f: &mut Frame, message: &str) {
    let area = centered_rect(50, 5, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Result ")
        .title_style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {}", message), TEXT_STYLE)),
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
