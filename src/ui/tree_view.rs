use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{
        Block, Borders, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
    },
    Frame,
};

use crate::process::{filter::ProcessKind, tree::FlatEntry};

use super::styles::{
    BORDER_STYLE, CHILD_STYLE, CLAUDE_COLOR, CODEX_COLOR, HEADER_STYLE, SELECTED_STYLE, TITLE_STYLE,
};

/// Column widths matching the spec.
const WIDTHS: [Constraint; 7] = [
    Constraint::Length(8),  // PID
    Constraint::Min(20),    // Name (with tree prefix)
    Constraint::Length(8),  // CPU%
    Constraint::Length(10), // Memory
    Constraint::Length(10), // Status
    Constraint::Min(30),    // Command
    Constraint::Length(12), // Uptime
];

/// Format a byte count as a human-readable string with one decimal place.
///
/// # Examples
///
/// ```
/// assert_eq!(format_memory(1_500), "1.5 KB");
/// assert_eq!(format_memory(2_097_152), "2.0 MB");
/// ```
fn format_memory(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    }
}

/// Format a duration in seconds as a compact human-readable string.
///
/// Produces "Xd Xh Xm" for durations >= 1 hour, or "Xm Xs" otherwise.
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let mins = (seconds % 3_600) / 60;
    let secs = seconds % 60;

    if days > 0 || hours > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else {
        format!("{}m {}s", mins, secs)
    }
}

/// Build the indentation and box-drawing connector prefix for a tree entry.
///
/// Each depth level adds two spaces of indentation. The immediate connector is
/// "└─ " for the last sibling in a group, or "├─ " otherwise.
fn tree_prefix(entry: &FlatEntry) -> String {
    // Reserve capacity: 2 chars per ancestor level + 3 for the connector.
    let mut prefix = String::with_capacity(entry.depth * 2 + 3);
    // Ancestors contribute indentation only (no connectors at this point).
    for _ in 0..entry.depth.saturating_sub(1) {
        prefix.push_str("  ");
    }
    if entry.depth > 0 {
        if entry.is_last_sibling {
            prefix.push_str("└─ ");
        } else {
            prefix.push_str("├─ ");
        }
    }
    prefix
}

/// Return the appropriate row style based on the entry's kind and root status.
fn row_style(entry: &FlatEntry) -> Style {
    match (&entry.kind, entry.is_root) {
        (Some(ProcessKind::Claude), true) => Style::new().fg(CLAUDE_COLOR),
        (Some(ProcessKind::Codex), true) => Style::new().fg(CODEX_COLOR),
        _ => CHILD_STYLE,
    }
}

/// Build the display name cell: tree prefix + expand indicator + process name.
fn name_cell(entry: &FlatEntry) -> String {
    let prefix = tree_prefix(entry);
    let indicator = if entry.has_children {
        if entry.expanded {
            "▼ "
        } else {
            "▶ "
        }
    } else {
        ""
    };
    format!("{}{}{}", prefix, indicator, entry.info.name)
}

/// Render the process tree as a bordered, scrollable [`Table`].
///
/// # Arguments
///
/// * `f`           - Ratatui frame.
/// * `area`        - Available screen area.
/// * `flat_list`   - Flattened, ordered list of visible tree entries.
/// * `table_state` - Mutable selection state (drives highlight and scrollbar).
pub fn render_tree_view(
    f: &mut Frame,
    area: Rect,
    flat_list: &[FlatEntry],
    table_state: &mut TableState,
) {
    let header = Row::new([
        "PID", "Name", "CPU%", "Memory", "Status", "Command", "Uptime",
    ])
    .style(HEADER_STYLE)
    .bottom_margin(1);

    let rows: Vec<Row> = flat_list
        .iter()
        .map(|entry| {
            let cmd = entry.info.cmd.join(" ");
            Row::new([
                entry.info.pid.to_string(),
                name_cell(entry),
                format!("{:.1}%", entry.info.cpu_usage),
                format_memory(entry.info.memory_bytes),
                entry.info.status.clone(),
                cmd,
                format_uptime(entry.info.run_time),
            ])
            .style(row_style(entry))
        })
        .collect();

    let block = Block::default()
        .title(" Process Inspector ")
        .title_style(TITLE_STYLE)
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);

    let table = Table::new(rows, WIDTHS)
        .header(header)
        .block(block)
        .row_highlight_style(SELECTED_STYLE)
        .highlight_symbol("> ");

    // Render the table with stateful selection.
    f.render_stateful_widget(table, area, table_state);

    // Overlay a vertical scrollbar on the right edge.
    let mut scroll_state =
        ScrollbarState::new(flat_list.len()).position(table_state.selected().unwrap_or(0));
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        area,
        &mut scroll_state,
    );
}
