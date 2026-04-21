use std::collections::HashMap;
use std::time::{Duration, Instant};

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
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
use crate::telemetry::{AgentStatus, AgentTelemetry, TelemetryMap};

use super::format::{format_duration_compact, format_memory};
use super::styles::{classify_idle, IdleTier, Palette};

/// Base view column widths (default view).
const BASE_WIDTHS: [Constraint; 7] = [
    Constraint::Length(8),  // PID
    Constraint::Min(20),    // Name (with tree prefix)
    Constraint::Length(16), // CPU% (+ aggregate for roots with children)
    Constraint::Length(18), // Memory (+ aggregate for roots with children)
    Constraint::Length(10), // Status
    Constraint::Min(30),    // Command
    Constraint::Length(12), // Uptime
];

/// Agent telemetry view column widths.
const AGENT_WIDTHS: [Constraint; 8] = [
    Constraint::Length(6),  // PID
    Constraint::Min(16),    // Name
    Constraint::Length(8),  // Status
    Constraint::Length(8),  // Ctx%
    Constraint::Length(9),  // Cost
    Constraint::Length(12), // Tokens
    Constraint::Length(12), // Tool
    Constraint::Length(8),  // Uptime
];

/// How many seconds of burn history to compare against for the cost ↑ indicator.
const BURN_WINDOW_SECS: u64 = 30;
/// Minimum cost increase in USD to trigger the burn indicator.
const BURN_THRESHOLD_USD: f64 = 0.10;

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
///
/// When `pending_tool` is `Some` in the associated telemetry, a `⚑` badge
/// is appended immediately after the activity badge.
fn name_cell<'a>(
    entry: &'a FlatEntry,
    palette: &Palette,
    telemetry: Option<&AgentTelemetry>,
) -> Line<'a> {
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

    let has_pending_tool = telemetry.and_then(|t| t.pending_tool.as_deref()).is_some();

    let mut spans: Vec<Span<'a>> = Vec::with_capacity(6);
    if !badge_text.is_empty() {
        spans.push(Span::styled(badge_text, badge_style(entry, palette)));
    }
    // Pending-tool badge: ⚑ in bold yellow.
    if has_pending_tool {
        spans.push(Span::styled(
            "\u{2691} ", // ⚑
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
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

/// Build the base-view table rows from a flattened process list.
///
/// Extracted from [`render_tree_view`] so each concern has a single home.
/// Root entries with children show aggregate CPU and memory in parentheses.
fn build_base_rows<'a>(
    flat_list: &'a [FlatEntry],
    palette: &Palette,
    telemetry: &TelemetryMap,
) -> Vec<Row<'a>> {
    flat_list
        .iter()
        .map(|entry| {
            let cmd = entry.info.cmd.join(" ");
            let tel = telemetry.get(&entry.info.pid);
            Row::new(vec![
                Cell::from(entry.info.pid.to_string()),
                Cell::from(name_cell(entry, palette, tel)),
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

/// Build the agent-view table rows.
///
/// Columns: PID | Name | Status | Ctx% | Cost | Tokens | Tool | Uptime.
/// Non-root or non-Claude entries show `–` in telemetry columns.
fn build_agent_rows<'a>(
    flat_list: &'a [FlatEntry],
    palette: &Palette,
    telemetry: &TelemetryMap,
    cost_burn: &HashMap<u32, (Instant, f64)>,
) -> Vec<Row<'a>> {
    flat_list
        .iter()
        .map(|entry| {
            let pid = entry.info.pid;
            let tel = telemetry.get(&pid);
            let row_st = row_style(entry, palette);

            // Status cell.
            let status_cell = tel
                .and_then(|t| t.status)
                .map(|s| {
                    let (label, style) = match s {
                        AgentStatus::Processing => (
                            "Active",
                            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
                        ),
                        AgentStatus::NeedsInput => (
                            "Needs In",
                            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                        ),
                        AgentStatus::WaitingInput => ("Waiting", Style::new().fg(Color::Cyan)),
                        AgentStatus::Idle => ("Idle", Style::new().fg(Color::Gray)),
                        AgentStatus::Finished => ("Done", Style::new().fg(Color::DarkGray)),
                        AgentStatus::Unknown => ("Unknown", Style::new().fg(Color::DarkGray)),
                    };
                    Cell::from(Span::styled(label, style))
                })
                .unwrap_or_else(|| Cell::from(Span::styled("\u{2013}", palette.dim_style())));

            // Ctx% cell.
            let ctx_cell = tel
                .and_then(|t| t.context_fraction())
                .map(|frac| {
                    let pct = (frac * 100.0) as u32;
                    let style = if pct < 50 {
                        Style::new().fg(Color::Green)
                    } else if pct < 80 {
                        Style::new().fg(Color::Yellow)
                    } else {
                        Style::new().fg(Color::Red)
                    };
                    Cell::from(Span::styled(format!("{pct}%"), style))
                })
                .unwrap_or_else(|| Cell::from(Span::styled("\u{2013}", palette.dim_style())));

            // Cost cell (with optional burn indicator).
            let cost_cell = tel
                .and_then(|t| t.cost_usd)
                .map(|cost| {
                    let style = if cost < 1.0 {
                        Style::new().fg(Color::Green)
                    } else if cost <= 5.0 {
                        Style::new().fg(Color::Yellow)
                    } else {
                        Style::new().fg(Color::Red)
                    };
                    // Check for burn rate: cost grew > $0.10 in last 30s.
                    let burning = cost_burn.get(&pid).is_some_and(|(ts, old_cost)| {
                        ts.elapsed() <= Duration::from_secs(BURN_WINDOW_SECS)
                            && (cost - old_cost) > BURN_THRESHOLD_USD
                    });
                    let burn_suffix = if burning {
                        Span::styled("\u{2191}", palette.dim_style()) // ↑
                    } else {
                        Span::raw("")
                    };
                    let cost_span = Span::styled(format!("${cost:.2}"), style);
                    Cell::from(Line::from(vec![cost_span, burn_suffix]))
                })
                .unwrap_or_else(|| Cell::from(Span::styled("\u{2013}", palette.dim_style())));

            // Tokens cell: "{input_k}k↑ {output_k}k↓"
            let tokens_cell = tel
                .and_then(|t| t.input_tokens.zip(t.output_tokens))
                .map(|(inp, out)| {
                    let text = format!("{}k\u{2191} {}k\u{2193}", inp / 1000, out / 1000);
                    Cell::from(Span::styled(text, Style::new().fg(Color::Gray)))
                })
                .unwrap_or_else(|| Cell::from(Span::styled("", palette.dim_style())));

            // Tool cell: truncated tool name, bold yellow when pending.
            let tool_cell = tel
                .and_then(|t| t.pending_tool.as_deref())
                .map(|tool| {
                    Cell::from(Span::styled(
                        super::format::truncate_chars(tool, 10),
                        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ))
                })
                .unwrap_or_else(|| {
                    // Show a dim gray label when there's telemetry but no pending tool.
                    let label = tel
                        .and_then(|t| t.status)
                        .map(|s| match s {
                            AgentStatus::Idle
                            | AgentStatus::WaitingInput
                            | AgentStatus::Finished => "idle",
                            _ => "",
                        })
                        .unwrap_or("");
                    Cell::from(Span::styled(label, palette.dim_style()))
                });

            Row::new(vec![
                Cell::from(entry.info.pid.to_string()),
                Cell::from(name_cell(entry, palette, tel)),
                status_cell,
                ctx_cell,
                cost_cell,
                tokens_cell,
                tool_cell,
                Cell::from(format_duration_compact(entry.info.run_time)),
            ])
            .style(row_st)
        })
        .collect()
}

/// Build base-view header labels with a sort indicator on the active column.
///
/// The `Command` column is not sortable; its slot holds `None` so the arrow
/// can never appear on it, avoiding the previous double-arrow bug.
fn base_header_labels(column: SortColumn, direction: SortDirection) -> Vec<String> {
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
            if col_opt.is_some_and(|c| c == column) {
                format!("{}{}", label, arrow)
            } else {
                label.to_string()
            }
        })
        .collect()
}

/// Build agent-view header labels.
///
/// Sort indicators are not shown in agent view — sorting falls back to the
/// current sort column where compatible.
fn agent_header_labels() -> Vec<String> {
    [
        "PID", "Name", "Status", "Ctx%", "Cost", "Tokens", "Tool", "Uptime",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Render the process tree as a bordered, scrollable [`Table`].
///
/// When `telemetry_view` is `true`, the table switches to the agent column set
/// (PID | Name | Status | Ctx% | Cost | Tokens | Tool | Uptime). The base
/// view (default) is unchanged.
///
/// # Arguments
///
/// * `f`              - Ratatui frame.
/// * `area`           - Available screen area.
/// * `flat_list`      - Flattened, ordered list of visible tree entries.
/// * `table_state`    - Mutable selection state (drives highlight and scrollbar).
/// * `sort_column`    - Currently active sort column.
/// * `sort_direction` - Current sort direction.
/// * `palette`        - Active color theme.
/// * `telemetry`      - Per-PID telemetry map from the provider pipeline.
/// * `telemetry_view` - When `true`, show agent telemetry columns.
/// * `cost_burn`      - Per-PID cost burn history for the burn indicator.
#[allow(clippy::too_many_arguments)]
pub fn render_tree_view(
    f: &mut Frame,
    area: Rect,
    flat_list: &[FlatEntry],
    table_state: &mut TableState,
    sort_column: SortColumn,
    sort_direction: SortDirection,
    palette: &Palette,
    telemetry: &TelemetryMap,
    telemetry_view: bool,
    cost_burn: &HashMap<u32, (Instant, f64)>,
) {
    let title = if telemetry_view {
        " agentop [T] Agent View "
    } else {
        " agentop "
    };

    let block = Block::default()
        .title(title)
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    if telemetry_view {
        let header = Row::new(agent_header_labels())
            .style(palette.header_style())
            .bottom_margin(1);
        let rows = build_agent_rows(flat_list, palette, telemetry, cost_burn);
        let table = Table::new(rows, AGENT_WIDTHS)
            .header(header)
            .block(block)
            .row_highlight_style(palette.selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(table, area, table_state);
    } else {
        let header = Row::new(base_header_labels(sort_column, sort_direction))
            .style(palette.header_style())
            .bottom_margin(1);
        let rows = build_base_rows(flat_list, palette, telemetry);
        let table = Table::new(rows, BASE_WIDTHS)
            .header(header)
            .block(block)
            .row_highlight_style(palette.selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(table, area, table_state);
    }

    // Overlay a vertical scrollbar on the right edge.
    let mut scroll_state =
        ScrollbarState::new(flat_list.len()).position(table_state.selected().unwrap_or(0));
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        area,
        &mut scroll_state,
    );
}
