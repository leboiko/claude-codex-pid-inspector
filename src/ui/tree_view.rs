use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
        TableState,
    },
    Frame,
};

use crate::app::{SortColumn, SortDirection};
use crate::process::tree::FlatEntry;
use crate::process::{display_name, ActivityState, ProcessKind};

use super::format::{format_duration_compact, format_memory};
use super::styles::{classify_idle, IdleTier, Palette};

/// Column widths. CPU and Memory are wider to accommodate aggregate `"self (total)"` display.
const WIDTHS: [Constraint; 7] = [
    Constraint::Length(8),  // PID
    Constraint::Min(20),    // Name (with tree prefix)
    Constraint::Length(16), // CPU% (+ aggregate for roots with children)
    Constraint::Length(18), // Memory (+ aggregate for roots with children)
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

/// Row style based on the entry's kind and root status.
///
/// Root processes keep their brand color (Claude orange, Codex green) so
/// users can identify the agent kind at a glance. The activity badge in
/// [`name_cell`] carries its own color independent of the row — per the
/// designer's "row color dominates; badge confirms" principle.
fn row_style(entry: &FlatEntry, palette: &Palette) -> Style {
    match (&entry.kind, entry.is_root) {
        (Some(ProcessKind::Claude), true) => palette.claude_style(),
        (Some(ProcessKind::Codex), true) => palette.codex_style(),
        _ => palette.child_style(),
    }
}

/// Style for the activity badge glyph, independent of the row style.
///
/// Idle badges escalate with time-in-state per the designer's spec:
/// Fresh → gray, Warning → yellow, Stale → red+bold. Active badges use a
/// dimmed green that confirms rather than competes with the row color.
fn badge_style(entry: &FlatEntry, palette: &Palette) -> Style {
    match entry.activity {
        Some(ActivityState::Active) => palette.activity_active_style(),
        Some(ActivityState::Idle) => {
            let tier = entry
                .activity_since
                .map(|t| classify_idle(t.elapsed()))
                .unwrap_or(IdleTier::Fresh);
            match tier {
                IdleTier::Fresh => palette.activity_idle_fresh_style(),
                IdleTier::Warning => palette.activity_idle_warning_style(),
                IdleTier::Stale => palette.activity_idle_stale_style(),
            }
        }
        Some(ActivityState::Unknown) | None => palette.activity_unknown_style(),
    }
}

/// Format the idle-duration suffix for Warning and Stale tiers.
///
/// Returns `""` for Fresh, `"(Xm)"` for 1–59 minutes, or `"(1h+)"` for ≥ 60 minutes.
fn idle_suffix(elapsed_secs: u64) -> String {
    let mins = elapsed_secs / 60;
    if mins == 0 {
        String::new()
    } else if mins < 60 {
        format!(" ({mins}m)")
    } else {
        " (1h+)".to_string()
    }
}

/// Build the display name cell as a multi-span line.
///
/// The activity badge is rendered in [`badge_style`] (independent of the row
/// style) so it can signal urgency without overriding the brand color of the
/// row. The remaining spans inherit the row style set by the table.
///
/// For idle root processes at Warning or Stale tier, a duration suffix is
/// appended to the badge text to make the state self-describing without
/// relying on color alone (accessibility).
fn name_cell<'a>(entry: &'a FlatEntry, palette: &Palette) -> Line<'a> {
    let prefix = tree_prefix(entry);
    let indicator = if entry.has_children {
        if entry.expanded {
            "\u{25bc} " // ▼
        } else {
            "\u{25b6} " // ▶
        }
    } else {
        ""
    };

    let badge_text = if entry.is_root {
        match entry.activity {
            Some(ActivityState::Active) => "\u{25cf} ".to_string(), // ●
            Some(ActivityState::Idle) => {
                let (tier, elapsed_secs) = entry
                    .activity_since
                    .map(|t| {
                        let e = t.elapsed();
                        (classify_idle(e), e.as_secs())
                    })
                    .unwrap_or((IdleTier::Fresh, 0));

                match tier {
                    IdleTier::Fresh => "\u{25cb} ".to_string(), // ○
                    IdleTier::Warning | IdleTier::Stale => {
                        format!("\u{25cb}{} ", idle_suffix(elapsed_secs)) // ○ (Xm)
                    }
                }
            }
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let mut spans: Vec<Span<'a>> = Vec::with_capacity(4);
    if !badge_text.is_empty() {
        spans.push(Span::styled(badge_text, badge_style(entry, palette)));
    }
    spans.push(Span::raw(prefix));
    spans.push(Span::raw(indicator));
    spans.push(Span::raw(display_name(&entry.info)));
    Line::from(spans)
}

/// Format the CPU cell for a flat entry.
///
/// For root nodes that have children, shows `"self% (agg%)"` so the user can
/// see both the process's own usage and the full subtree footprint at a glance.
fn cpu_cell(entry: &FlatEntry) -> String {
    let self_cpu = format!("{:.1}%", entry.info.cpu_usage);
    if entry.is_root && entry.has_children {
        format!("{} ({:.1}%)", self_cpu, entry.subtree_stats.total_cpu)
    } else {
        self_cpu
    }
}

/// Format the memory cell for a flat entry.
///
/// For root nodes that have children, shows `"self (agg)"` so the user can
/// see both the process's own memory and the full subtree footprint.
fn memory_cell(entry: &FlatEntry) -> String {
    let self_mem = format_memory(entry.info.memory_bytes);
    if entry.is_root && entry.has_children {
        format!(
            "{} ({})",
            self_mem,
            format_memory(entry.subtree_stats.total_memory)
        )
    } else {
        self_mem
    }
}

/// Build the table rows from a flattened process list.
///
/// Extracted from [`render_tree_view`] so each concern has a single home.
/// Root entries with children show aggregate CPU and memory in parentheses.
fn build_rows<'a>(flat_list: &'a [FlatEntry], palette: &Palette) -> Vec<Row<'a>> {
    flat_list
        .iter()
        .map(|entry| {
            let cmd = entry.info.cmd.join(" ");
            Row::new(vec![
                Cell::from(entry.info.pid.to_string()),
                Cell::from(name_cell(entry, palette)),
                Cell::from(cpu_cell(entry)),
                Cell::from(memory_cell(entry)),
                Cell::from(entry.info.status.clone()),
                Cell::from(cmd),
                Cell::from(format_duration_compact(entry.info.run_time)),
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
    let base = [
        "PID", "Name", "CPU%", "Memory", "Status", "Command", "Uptime",
    ];
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
