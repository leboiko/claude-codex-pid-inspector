use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{
        Block, Borders, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
    },
    Frame,
};

use crate::app::{SortColumn, SortDirection};
use crate::process::ProcessKind;
use crate::process::tree::FlatEntry;

use super::format::{format_duration_compact, format_memory};
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
    let display_name = if entry.is_root {
        match &entry.kind {
            Some(ProcessKind::Claude) => "claude",
            Some(ProcessKind::Codex) => "codex",
            _ => &entry.info.name,
        }
    } else {
        &entry.info.name
    };
    format!("{}{}{}", prefix, indicator, display_name)
}

/// Build the table rows from a flattened process list.
///
/// Extracted from [`render_tree_view`] so each concern has a single home.
fn build_rows(flat_list: &[FlatEntry]) -> Vec<Row<'_>> {
    flat_list
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
                format_duration_compact(entry.info.run_time),
            ])
            .style(row_style(entry))
        })
        .collect()
}

/// Build header labels with a sort indicator on the active column.
///
/// The `Command` column is not sortable; its slot holds `None` so the arrow
/// can never appear on it, avoiding the previous double-arrow bug.
fn header_labels(column: SortColumn, direction: SortDirection) -> Vec<String> {
    let arrow = match direction {
        SortDirection::Ascending => " ^",
        SortDirection::Descending => " v",
    };
    let base = ["PID", "Name", "CPU%", "Memory", "Status", "Command", "Uptime"];
    // `None` marks columns that are not sortable (Command).
    let sort_cols: [Option<SortColumn>; 7] = [
        Some(SortColumn::Pid),
        Some(SortColumn::Name),
        Some(SortColumn::Cpu),
        Some(SortColumn::Memory),
        Some(SortColumn::Status),
        None, // Command is not sortable
        Some(SortColumn::Uptime),
    ];
    base.iter()
        .zip(sort_cols.iter())
        .map(|(label, col_opt)| {
            // `map_or` returns false when `col_opt` is None, safely skipping unsortable columns.
            if col_opt.is_some_and(|c| c == column) {
                format!("{}{}", label, arrow)
            } else {
                label.to_string()
            }
        })
        .collect()
}

/// Render the process tree as a bordered, scrollable [`Table`].
///
/// # Arguments
///
/// * `f`              - Ratatui frame.
/// * `area`           - Available screen area.
/// * `flat_list`      - Flattened, ordered list of visible tree entries.
/// * `table_state`    - Mutable selection state (drives highlight and scrollbar).
/// * `sort_column`    - Currently active sort column.
/// * `sort_direction` - Current sort direction.
pub fn render_tree_view(
    f: &mut Frame,
    area: Rect,
    flat_list: &[FlatEntry],
    table_state: &mut TableState,
    sort_column: SortColumn,
    sort_direction: SortDirection,
) {
    let header = Row::new(header_labels(sort_column, sort_direction))
        .style(HEADER_STYLE)
        .bottom_margin(1);

    let rows = build_rows(flat_list);

    let block = Block::default()
        .title(" agentop ")
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
