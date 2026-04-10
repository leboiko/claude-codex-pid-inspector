use std::path::Path;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, Wrap},
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

/// Render a time-series chart with dot markers inside a bordered block.
///
/// The X axis is the sample index (oldest on the left, newest on the right)
/// and the Y axis auto-scales from 0 to the observed maximum, with the max
/// value shown as a label in the top-left corner of the plot area.
///
/// # Arguments
///
/// * `f`        - Ratatui frame.
/// * `area`     - Target area.
/// * `title`    - Block title (usually contains the current value).
/// * `points`   - Series of `(x, y)` data points.
/// * `y_max`    - Upper bound for the Y axis.
/// * `y_labels` - Axis tick labels for the Y axis (typically `["0", "max"]`).
/// * `style`    - Dot color style.
fn render_chart(
    f: &mut Frame,
    area: Rect,
    title: &str,
    points: &[(f64, f64)],
    y_max: f64,
    y_labels: Vec<String>,
    style: Style,
) {
    let dataset = Dataset::default()
        .data(points)
        .marker(Marker::Dot)
        .graph_type(GraphType::Scatter)
        .style(style);

    // X axis spans the full history length so the plot anchors to the right
    // edge regardless of how many samples have been collected so far.
    let x_max = (points.len().saturating_sub(1)).max(1) as f64;
    let x_axis = Axis::default().bounds([0.0, x_max]);

    let y_axis = Axis::default()
        .bounds([0.0, y_max])
        .labels(y_labels)
        .style(Style::new().fg(Color::Yellow));

    let block = Block::default()
        .title(title)
        .title_style(TITLE_STYLE)
        .borders(Borders::ALL)
        .border_style(BORDER_STYLE);

    let chart = Chart::new(vec![dataset])
        .block(block)
        .x_axis(x_axis)
        .y_axis(y_axis);

    f.render_widget(chart, area);
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

/// Extract the basename of a path as a `&str`, falling back to `"—"`.
fn basename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("—")
}

/// Read the current branch name from a `.git/HEAD` file inside `cwd`.
///
/// Returns `None` if the directory is not a git repo or the HEAD file
/// can't be parsed. Handles both symbolic refs (`ref: refs/heads/master`)
/// and detached HEAD (returns the short SHA).
fn git_branch(cwd: &str) -> Option<String> {
    let head = std::fs::read_to_string(Path::new(cwd).join(".git/HEAD")).ok()?;
    let trimmed = head.trim();
    if let Some(rest) = trimmed.strip_prefix("ref: refs/heads/") {
        Some(rest.to_string())
    } else if trimmed.len() >= 7 {
        // Detached HEAD: show short SHA.
        Some(format!("({})", &trimmed[..7]))
    } else {
        None
    }
}

/// Build key-value info lines from a [`ProcessInfo`] and render them as a [`Paragraph`].
fn render_info_table(f: &mut Frame, area: Rect, info: &ProcessInfo) {
    // Build all formatted values as owned Strings first, then borrow them
    // as &str for `kv`. This avoids passing temporaries by value into the
    // Span, which requires `'static` lifetimes in ratatui's Cow-based API.
    let parent = info.parent_pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into());
    let exe = info.exe_path.as_deref().unwrap_or("—");
    let cwd = info.cwd.as_deref().unwrap_or("—");
    let project = info.cwd.as_deref().map(basename).unwrap_or("—");
    let branch = info
        .cwd
        .as_deref()
        .and_then(git_branch)
        .unwrap_or_else(|| "—".into());
    let env_str = info.environ_count.to_string();
    let pid_str = info.pid.to_string();
    let runtime_str = format_duration_full(info.run_time);

    let lines = vec![
        kv("PID:            ", &pid_str),
        kv("Parent PID:     ", &parent),
        kv("Name:           ", display_name(info)),
        kv("Status:         ", &info.status),
        kv("Project:        ", project),
        kv("Git Branch:     ", &branch),
        kv("Working Dir:    ", cwd),
        kv("Exe Path:       ", exe),
        kv("Env Vars:       ", &env_str),
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
            Constraint::Length(12), // Info table
            Constraint::Fill(1),    // CPU sparkline (splits remaining space with memory)
            Constraint::Fill(1),    // Memory sparkline
            Constraint::Length(4),  // Command
        ])
        .split(area);

    render_header(f, sections[0], info);
    render_info_table(f, sections[1], info);

    // --- CPU chart -----------------------------------------------------------
    let cpu_points: Vec<(f64, f64)> = cpu_history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v as f64))
        .collect();
    // Observed max, padded by 10% and floored at 1% so the chart doesn't
    // collapse to a flat line when usage is near zero.
    let cpu_observed_max = cpu_history.iter().copied().fold(0.0_f32, f32::max) as f64;
    let cpu_axis_max = (cpu_observed_max * 1.15).max(1.0);
    let cpu_labels = vec!["0".to_string(), format!("{:.1}%", cpu_observed_max)];
    let cpu_title = format!(" cpu {:.2}% ", info.cpu_usage);
    render_chart(
        f,
        sections[2],
        &cpu_title,
        &cpu_points,
        cpu_axis_max,
        cpu_labels,
        CLAUDE_SPARKLINE_STYLE,
    );

    // --- Memory chart --------------------------------------------------------
    let mem_points: Vec<(f64, f64)> = mem_history
        .iter()
        .enumerate()
        .map(|(i, &b)| (i as f64, b as f64 / (1024.0 * 1024.0)))
        .collect();
    let mem_observed_max = mem_history.iter().copied().max().unwrap_or(0) as f64 / (1024.0 * 1024.0);
    let mem_axis_max = (mem_observed_max * 1.15).max(1.0);
    let mem_labels = vec!["0".to_string(), format!("{:.1} MB", mem_observed_max)];
    let mem_title = format!(" memory {} ", format_memory(info.memory_bytes));
    render_chart(
        f,
        sections[3],
        &mem_title,
        &mem_points,
        mem_axis_max,
        mem_labels,
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
