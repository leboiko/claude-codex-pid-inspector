use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Sparkline, Wrap},
    Frame,
};

use crate::process::info::ProcessInfo;

use super::styles::{BORDER_STYLE, TITLE_STYLE};

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

/// Build key-value info lines from a [`ProcessInfo`] and render them as a [`Paragraph`].
fn render_info_table(f: &mut Frame, area: Rect, info: &ProcessInfo) {
    /// Helper: build a `Line` with a styled key and a plain value.
    fn kv<'a>(key: &'a str, val: String) -> Line<'a> {
        Line::from(vec![
            Span::styled(key, KEY_STYLE),
            Span::styled(val, VAL_STYLE),
        ])
    }

    let parent = info
        .parent_pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| "—".to_string());
    let exe = info.exe_path.clone().unwrap_or_else(|| "—".to_string());
    let cwd = info.cwd.clone().unwrap_or_else(|| "—".to_string());

    let lines = vec![
        kv("PID:            ", info.pid.to_string()),
        kv("Parent PID:     ", parent),
        kv("Name:           ", info.name.clone()),
        kv("Status:         ", info.status.clone()),
        kv("CPU:            ", format!("{:.1}%", info.cpu_usage)),
        kv("Memory:         ", format_memory(info.memory_bytes)),
        kv("Exe Path:       ", exe),
        kv("Working Dir:    ", cwd),
        kv("Env Vars:       ", info.environ_count.to_string()),
        kv("Started:        ", format!("epoch {}", info.start_time)),
        kv("Run Time:       ", format_run_time(info.run_time)),
    ];

    let block = Block::default()
        .title(" Info ")
        .title_style(TITLE_STYLE)
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Format a byte count as a human-readable string.
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

/// Format a run-time in seconds as "Xd Xh Xm Xs".
fn format_run_time(seconds: u64) -> String {
    let d = seconds / 86_400;
    let h = (seconds % 86_400) / 3_600;
    let m = (seconds % 3_600) / 60;
    let s = seconds % 60;
    format!("{}d {}h {}m {}s", d, h, m, s)
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

    // Multiply CPU float (0–100) by 10 to keep one decimal of precision as u64.
    let cpu_data: Vec<u64> = cpu_history.iter().map(|&v| (v * 10.0) as u64).collect();
    render_sparkline(
        f,
        sections[2],
        " CPU Usage % ",
        &cpu_data,
        // CLAUDE_COLOR is Color::Rgb(204, 120, 50); duplicate the literal here
        // because Color is an enum with no accessor methods.
        Style::new().fg(Color::Rgb(204, 120, 50)),
    );

    // Convert bytes to MB for a readable scale in the sparkline.
    let mem_data: Vec<u64> = mem_history.iter().map(|&b| b / (1024 * 1024)).collect();
    render_sparkline(
        f,
        sections[3],
        " Memory Usage (MB) ",
        &mem_data,
        // CODEX_COLOR is Color::Rgb(100, 200, 100).
        Style::new().fg(Color::Rgb(100, 200, 100)),
    );

    render_command(f, sections[4], info);
}

/// Render a compact header block showing the process name, PID, and status.
fn render_header(f: &mut Frame, area: Rect, info: &ProcessInfo) {
    let title = format!(" {} — PID {} — {} ", info.name, info.pid, info.status);
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
