use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::format::format_memory;
use crate::app::{AgentSummary, FocusFilter};
use crate::process::SystemStats;

const LABEL_STYLE: Style = Style::new().fg(Color::DarkGray);
const VALUE_STYLE: Style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

/// Render a one-line system resource bar showing CPU, memory, agent summary,
/// filter pill, and any transient status message.
///
/// The filter pill is always visible so users know what lens is active.
/// When a transient `status_message` is provided it replaces the pill for the
/// duration of its 3-second TTL.
///
/// # Arguments
///
/// * `f`               - Ratatui frame.
/// * `area`            - One-line area reserved for the status bar.
/// * `stats`           - System-wide CPU and memory snapshot.
/// * `agent_summary`   - Aggregated counts and resource usage for agent processes.
/// * `focus_filter`    - The currently active curated focus filter.
/// * `filter_text`     - Current free-text filter query (empty when not active).
/// * `visible_count`   - Number of process rows currently visible (post-filter).
/// * `total_count`     - Total number of process rows in the forest (pre-filter).
/// * `status_message`  - Optional transient flash message (e.g. jump result).
#[allow(clippy::too_many_arguments)]
pub fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    stats: &SystemStats,
    agent_summary: &AgentSummary,
    focus_filter: FocusFilter,
    filter_text: &str,
    visible_count: usize,
    total_count: usize,
    status_message: Option<&str>,
) {
    let mem_used = format_memory(stats.used_memory);
    let mem_total = format_memory(stats.total_memory);
    let mem_pct = if stats.total_memory > 0 {
        (stats.used_memory as f64 / stats.total_memory as f64) * 100.0
    } else {
        0.0
    };

    let mut spans = vec![
        Span::styled(" CPU: ", LABEL_STYLE),
        Span::styled(
            format!("{:.1}%", stats.cpu_usage),
            cpu_color(stats.cpu_usage),
        ),
        Span::styled(format!(" ({} cores)", stats.cpu_count), LABEL_STYLE),
        Span::styled("  |  Mem: ", LABEL_STYLE),
        Span::styled(format!("{}/{}", mem_used, mem_total), mem_color(mem_pct)),
        Span::styled(format!(" ({:.0}%)", mem_pct), mem_color(mem_pct)),
    ];

    if stats.total_swap > 0 {
        let swap_used = format_memory(stats.used_swap);
        let swap_total = format_memory(stats.total_swap);
        spans.push(Span::styled("  |  Swap: ", LABEL_STYLE));
        spans.push(Span::styled(
            format!("{}/{}", swap_used, swap_total),
            VALUE_STYLE,
        ));
    }

    // Append agent section only when at least one agent is running.
    let total_agents = agent_summary.claude_count + agent_summary.codex_count;
    if total_agents > 0 {
        let agent_label = match (agent_summary.claude_count, agent_summary.codex_count) {
            (c, 0) => format!("{} Claude", c),
            (0, d) => format!("{} Codex", d),
            (c, d) => format!("{} Claude, {} Codex", c, d),
        };

        spans.push(Span::styled("  |  Agents: ", LABEL_STYLE));
        spans.push(Span::styled(agent_label, VALUE_STYLE));
        spans.push(Span::styled("  RAM: ", LABEL_STYLE));
        spans.push(Span::styled(
            format_memory(agent_summary.total_memory),
            VALUE_STYLE,
        ));
        spans.push(Span::styled("  CPU: ", LABEL_STYLE));
        spans.push(Span::styled(
            format!("{:.1}%", agent_summary.total_cpu),
            VALUE_STYLE,
        ));
    }

    // Filter pill — always present.
    spans.push(Span::styled("  |  ", LABEL_STYLE));
    if let Some(msg) = status_message {
        // Transient status message overrides the pill.
        spans.push(Span::styled(
            msg.to_string(),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    } else {
        let pill_label = filter_pill_label(focus_filter, filter_text);
        spans.push(Span::styled(
            "FILTER: ",
            Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            pill_label,
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("  ({}/{})", visible_count, total_count),
            LABEL_STYLE,
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Build the label portion of the filter pill.
///
/// - `FILTER: all` when no filter is active.
/// - `FILTER: <name>` for curated filters.
/// - `FILTER: /<text>` for free-text search.
fn filter_pill_label(focus_filter: FocusFilter, filter_text: &str) -> String {
    if !filter_text.is_empty() {
        return format!("/{filter_text}");
    }
    focus_filter.label().to_string()
}

/// Color CPU usage: green < 50%, yellow < 80%, red >= 80%.
fn cpu_color(pct: f32) -> Style {
    let color = if pct < 50.0 {
        Color::Green
    } else if pct < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    Style::new().fg(color).add_modifier(Modifier::BOLD)
}

/// Color memory usage: green < 60%, yellow < 85%, red >= 85%.
fn mem_color(pct: f64) -> Style {
    let color = if pct < 60.0 {
        Color::Green
    } else if pct < 85.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    Style::new().fg(color).add_modifier(Modifier::BOLD)
}
