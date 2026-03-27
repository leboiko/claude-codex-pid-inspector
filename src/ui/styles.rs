use ratatui::style::{Color, Modifier, Style};

/// Style for column headers and section titles.
pub const HEADER_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);

/// Style for the currently selected/highlighted row.
pub const SELECTED_STYLE: Style = Style::new()
    .bg(Color::DarkGray)
    .add_modifier(Modifier::BOLD);

/// Foreground color for Claude root processes (orange).
pub const CLAUDE_COLOR: Color = Color::Rgb(204, 120, 50);

/// Foreground color for Codex root processes (green).
pub const CODEX_COLOR: Color = Color::Rgb(100, 200, 100);

/// Style for child (non-root) process rows.
pub const CHILD_STYLE: Style = Style::new().fg(Color::Gray);

/// Style for widget borders.
pub const BORDER_STYLE: Style = Style::new().fg(Color::DarkGray);

/// Style for block/widget titles.
pub const TITLE_STYLE: Style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);
