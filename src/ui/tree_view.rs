use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{
        Block, Borders, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
    },
    Frame,
};

use crate::app::{SortColumn, SortDirection};
use crate::process::tree::FlatEntry;
use crate::process::{display_name, ProcessKind};

use super::format::{format_duration_compact, format_memory};
use super::styles::Palette;

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
fn row_style(entry: &FlatEntry, palette: &Palette) -> Style {
    match (&entry.kind, entry.is_root) {
        (Some(ProcessKind::Claude), true) => palette.claude_style(),
        (Some(ProcessKind::Codex), true) => palette.codex_style(),
        _ => palette.child_style(),
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
    format!("{}{}{}", prefix, indicator, display_name(&entry.info))
}

/// Build the table rows from a flattened process list.
///
/// Extracted from [`render_tree_view`] so each concern has a single home.
fn build_rows<'a>(flat_list: &'a [FlatEntry], palette: &Palette) -> Vec<Row<'a>> {
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
            .style(row_style(entry, palette))
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
    palette: &Palette,
) {
    let header = Row::new(header_labels(sort_column, sort_direction))
        .style(palette.header_style())
        .bottom_margin(1);

    let rows = build_rows(flat_list, palette);

    let block = Block::default()
        .title(" agentop ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    let table = Table::new(rows, WIDTHS)
        .header(header)
        .block(block)
        .row_highlight_style(palette.selected_style())
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
