use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::ConfigPopupState;

use super::styles::{GraphStyle, Palette, Theme};

/// Filled bullet shown next to the option that is currently applied.
const ACTIVE_BULLET: &str = "●";
/// Hollow bullet for inactive options.
const INACTIVE_BULLET: &str = "○";

/// Render a centered settings popup allowing the user to pick a graph style
/// and a color theme.
///
/// The popup captures keyboard focus while open (see
/// [`crate::app::App::map_key_to_action`]): `Up`/`Down` (or `k`/`j`) move the
/// cursor, `Enter` applies the highlighted option, and `Esc` or `c` closes it.
///
/// # Arguments
///
/// * `f`            - Ratatui frame.
/// * `state`        - Cursor position within the flat option list.
/// * `graph_style`  - Currently-applied graph style (shown with a filled bullet).
/// * `theme`        - Currently-applied theme (shown with a filled bullet).
/// * `palette`      - Active palette used for borders and accents.
pub fn render_config_popup(
    f: &mut Frame,
    state: &ConfigPopupState,
    graph_style: GraphStyle,
    theme: Theme,
    palette: &Palette,
) {
    // Popup dimensions: wide enough for the longest label plus room for
    // the bullet and cursor; tall enough for both sections and a help line.
    let area = centered_rect(44, 14, f.area());

    // Clear the background behind the popup so table rows don't bleed through.
    f.render_widget(Clear, area);

    let lines = build_lines(state, graph_style, theme, palette);

    let block = Block::default()
        .title(" Settings ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Build the content lines for the popup body.
fn build_lines<'a>(
    state: &ConfigPopupState,
    graph_style: GraphStyle,
    theme: Theme,
    palette: &Palette,
) -> Vec<Line<'a>> {
    let section_style = palette.header_style();
    let text_style = Style::new().fg(Color::White);
    let cursor_style = Style::new()
        .fg(palette.claude)
        .add_modifier(Modifier::BOLD);
    let dim_style = Style::new().fg(Color::DarkGray);
    let active_style = Style::new()
        .fg(palette.codex)
        .add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    let mut row_counter = 0usize;

    // --- Graph Style section -------------------------------------------------
    lines.push(Line::from(vec![
        Span::styled("  ", text_style),
        Span::styled(ConfigPopupState::SECTIONS[0].0, section_style),
    ]));
    for option in GraphStyle::ALL {
        let is_cursor = row_counter == state.cursor;
        let is_active = option == graph_style;
        lines.push(option_line(
            is_cursor,
            is_active,
            option.label(),
            cursor_style,
            active_style,
            text_style,
            dim_style,
        ));
        row_counter += 1;
    }

    lines.push(Line::from(""));

    // --- Theme section -------------------------------------------------------
    lines.push(Line::from(vec![
        Span::styled("  ", text_style),
        Span::styled(ConfigPopupState::SECTIONS[1].0, section_style),
    ]));
    for option in Theme::ALL {
        let is_cursor = row_counter == state.cursor;
        let is_active = option == theme;
        lines.push(option_line(
            is_cursor,
            is_active,
            option.label(),
            cursor_style,
            active_style,
            text_style,
            dim_style,
        ));
        row_counter += 1;
    }

    lines.push(Line::from(""));

    // --- Help footer ---------------------------------------------------------
    lines.push(Line::from(vec![
        Span::styled("  ", text_style),
        Span::styled("↑↓", cursor_style),
        Span::styled(" Navigate  ", dim_style),
        Span::styled("Enter", cursor_style),
        Span::styled(" Apply  ", dim_style),
        Span::styled("Esc/c", cursor_style),
        Span::styled(" Close", dim_style),
    ]));

    lines
}

/// Build a single option row with the cursor arrow, bullet, and label.
#[allow(clippy::too_many_arguments)]
fn option_line<'a>(
    is_cursor: bool,
    is_active: bool,
    label: &'a str,
    cursor_style: Style,
    active_style: Style,
    text_style: Style,
    dim_style: Style,
) -> Line<'a> {
    let arrow = if is_cursor { "> " } else { "  " };
    let bullet = if is_active { ACTIVE_BULLET } else { INACTIVE_BULLET };
    let bullet_style = if is_active { active_style } else { dim_style };
    let label_style = if is_cursor { cursor_style } else { text_style };

    Line::from(vec![
        Span::styled("  ", text_style),
        Span::styled(arrow, cursor_style),
        Span::styled(bullet, bullet_style),
        Span::styled(" ", text_style),
        Span::styled(label, label_style),
    ])
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
