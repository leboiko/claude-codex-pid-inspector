use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Sparkline, Wrap},
    Frame,
};

use crate::process::display_name;
use crate::process::info::ProcessInfo;

use super::format::{format_duration_full, format_memory};
use super::styles::{
    BORDER_STYLE, CLAUDE_SPARKLINE_STYLE, CODEX_SPARKLINE_STYLE, TITLE_STYLE,
};

/// Style for info-table key labels (e.g. "PID:").
const KEY_STYLE: Style = Style::new().fg(Color::Yellow);

/// Style for info-table value text.
const VAL_STYLE: Style = Style::new().fg(Color::White);

/// Render a labeled sparkline chart inside a bordered block.
///
/// # Arguments
///
/// * `f`     - Ratatui frame.
/// * `area`  - Target area.
/// * `title` - Block title string.
/// * `data`  - Series of u64 data points (most-recent last).
/// * `style` - Bar color style for the sparkline.
fn render_sparkline(f: &mut Frame, area: Rect, title: &str, data: &[u64], style: Style) {
    let block = Block::default()
        .title(title)
        .title_style(TITLE_STYLE)
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);

    let sparkline = Sparkline::default().block(block).data(data).style(style);
    f.render_widget(sparkline, area);
}

/// Helper: build a `Line` with a styled key and a plain value.
///
/// Both `key` and `val` are borrowed for the lifetime `'a`, avoiding
/// unnecessary allocations at the call site.
fn kv<'a>(key: &'a str, val: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(key, KEY_STYLE),
        Span::styled(val, VAL_STYLE),
    ])
}

/// Build key-value info lines from a [`ProcessInfo`] and render them as a [`Paragraph`].
fn render_info_table(f: &mut Frame, area: Rect, info: &ProcessInfo) {
    // Build all formatted values as owned Strings first, then borrow them
    // as &str for `kv`. This avoids passing temporaries by value into the
    // Span, which requires `'static` lifetimes in ratatui's Cow-based API.
    let parent = info.parent_pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into());
    let exe = info.exe_path.as_deref().unwrap_or("—");
    let cwd = info.cwd.as_deref().unwrap_or("—");
    let cpu_str = format!("{:.1}%", info.cpu_usage);
    let mem_str = format_memory(info.memory_bytes);
    let env_str = info.environ_count.to_string();
    let pid_str = info.pid.to_string();
    let started_str = format!("epoch {}", info.start_time);
    let runtime_str = format_duration_full(info.run_time);

    let lines = vec![
        kv("PID:            ", &pid_str),
        kv("Parent PID:     ", &parent),
        kv("Name:           ", display_name(info)),
        kv("Status:         ", &info.status),
        kv("CPU:            ", &cpu_str),
        kv("Memory:         ", &mem_str),
        kv("Exe Path:       ", exe),
        kv("Working Dir:    ", cwd),
        kv("Env Vars:       ", &env_str),
        kv("Started:        ", &started_str),
        kv("Run Time:       ", &runtime_str),
    ];

    let block = Block::default()
        .title(" Info ")
        .title_style(TITLE_STYLE)
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Render the full detail view for a selected process.
///
/// The area is split vertically into five sections:
/// header, info table, CPU sparkline, memory sparkline, and command.
///
/// # Arguments
///
/// * `f`           - Ratatui frame.
/// * `area`        - Available screen area.
/// * `info`        - Process information to display.
/// * `cpu_history` - Recent CPU usage samples (0.0–100.0).
/// * `mem_history` - Recent memory usage samples in bytes.
pub fn render_detail_view(
    f: &mut Frame,
    area: Rect,
    info: &ProcessInfo,
    cpu_history: &[f32],
    mem_history: &[u64],
) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(13), // Info table
            Constraint::Length(4),  // CPU sparkline
            Constraint::Length(4),  // Memory sparkline
            Constraint::Min(3),     // Command
        ])
        .split(area);

    render_header(f, sections[0], info);
    render_info_table(f, sections[1], info);

    // Multiply CPU float (0–100) by 10 to preserve one decimal of precision as u64.
    let cpu_data: Vec<u64> = cpu_history.iter().map(|&v| (v * 10.0) as u64).collect();
    render_sparkline(f, sections[2], " CPU Usage % ", &cpu_data, CLAUDE_SPARKLINE_STYLE);

    // Convert bytes to MB for a readable scale in the sparkline.
    let mem_data: Vec<u64> = mem_history.iter().map(|&b| b / (1024 * 1024)).collect();
    render_sparkline(
        f,
        sections[3],
        " Memory Usage (MB) ",
        &mem_data,
        CODEX_SPARKLINE_STYLE,
    );

    render_command(f, sections[4], info);
}

/// Render a compact header block showing the process name, PID, and status.
fn render_header(f: &mut Frame, area: Rect, info: &ProcessInfo) {
    let title = format!(" {} — PID {} — {} ", display_name(info), info.pid, info.status);
    let block = Block::default()
        .title(title)
        .title_style(TITLE_STYLE.add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);
    f.render_widget(block, area);
}

/// Render the full command line in a wrapped paragraph block.
fn render_command(f: &mut Frame, area: Rect, info: &ProcessInfo) {
    let cmd_text = info.cmd.join(" ");
    let block = Block::default()
        .title(" Command ")
        .title_style(TITLE_STYLE)
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);

    f.render_widget(
        Paragraph::new(cmd_text)
            .block(block)
            .style(Style::new().fg(Color::White))
            .wrap(Wrap { trim: false }),
        area,
    );
}
