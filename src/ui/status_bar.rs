use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::format::format_memory;
use crate::app::AgentSummary;
use crate::process::SystemStats;

const LABEL_STYLE: Style = Style::new().fg(Color::DarkGray);
const VALUE_STYLE: Style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

/// Render a one-line system resource bar showing CPU, memory, and agent summary.
///
/// The agent section (Agents, RAM, CPU) is only appended when at least one
/// agent process (Claude or Codex) is running, keeping the bar uncluttered
/// when no agents are active.
///
/// # Arguments
///
/// * `f`             - Ratatui frame.
/// * `area`          - One-line area reserved for the status bar.
/// * `stats`         - System-wide CPU and memory snapshot.
/// * `agent_summary` - Aggregated counts and resource usage for agent processes.
pub fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    stats: &SystemStats,
    agent_summary: &AgentSummary,
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
        // Build the agent label, e.g. "8 Claude, 1 Codex" or "3 Claude".
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

    f.render_widget(Paragraph::new(Line::from(spans)), area);
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
