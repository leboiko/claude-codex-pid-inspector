use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::process::SystemStats;
use super::format::format_memory;

const LABEL_STYLE: Style = Style::new().fg(Color::DarkGray);
const VALUE_STYLE: Style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

/// Render a one-line system resource bar showing CPU and memory usage.
pub fn render_status_bar(f: &mut Frame, area: Rect, stats: &SystemStats) {
    let mem_used = format_memory(stats.used_memory);
    let mem_total = format_memory(stats.total_memory);
    let mem_pct = if stats.total_memory > 0 {
        (stats.used_memory as f64 / stats.total_memory as f64) * 100.0
    } else {
        0.0
    };

    let mut spans = vec![
        Span::styled(" CPU: ", LABEL_STYLE),
        Span::styled(format!("{:.1}%", stats.cpu_usage), cpu_color(stats.cpu_usage)),
        Span::styled(format!(" ({} cores)", stats.cpu_count), LABEL_STYLE),
        Span::styled("  |  Mem: ", LABEL_STYLE),
        Span::styled(format!("{}/{}", mem_used, mem_total), mem_color(mem_pct)),
        Span::styled(format!(" ({:.0}%)", mem_pct), mem_color(mem_pct)),
    ];

    if stats.total_swap > 0 {
        let swap_used = format_memory(stats.used_swap);
        let swap_total = format_memory(stats.total_swap);
        spans.push(Span::styled("  |  Swap: ", LABEL_STYLE));
        spans.push(Span::styled(format!("{}/{}", swap_used, swap_total), VALUE_STYLE));
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
