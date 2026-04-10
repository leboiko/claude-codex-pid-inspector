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
use super::styles::{GraphStyle, Palette};

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
#[allow(clippy::too_many_arguments)]
fn render_chart(
    f: &mut Frame,
    area: Rect,
    title: &str,
    points: &[(f64, f64)],
    y_max: f64,
    y_labels: Vec<String>,
    style: Style,
    graph_style: GraphStyle,
    palette: &Palette,
) {
    // Swap marker + graph type based on the user's preference. The Chart
    // widget supports both representations out of the box; we just pick the
    // right combination so the visual is scatter dots vs. vertical bars.
    let (marker, graph_type) = match graph_style {
        GraphStyle::Dots => (Marker::Dot, GraphType::Scatter),
        GraphStyle::Bars => (Marker::Bar, GraphType::Bar),
    };

    let dataset = Dataset::default()
        .data(points)
        .marker(marker)
        .graph_type(graph_type)
        .style(style);

    // X axis spans the full history length so the plot anchors to the right
    // edge regardless of how many samples have been collected so far.
    let x_max = (points.len().saturating_sub(1)).max(1) as f64;
    let x_axis = Axis::default().bounds([0.0, x_max]);

    let y_axis = Axis::default()
        .bounds([0.0, y_max])
        .labels(y_labels)
        .style(palette.label_style());

    let block = Block::default()
        .title(title)
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

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
fn kv<'a>(key: &'a str, val: &'a str, palette: &Palette) -> Line<'a> {
    Line::from(vec![
        Span::styled(key, palette.label_style()),
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
fn render_info_table(f: &mut Frame, area: Rect, info: &ProcessInfo, palette: &Palette) {
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
        kv("PID:            ", &pid_str, palette),
        kv("Parent PID:     ", &parent, palette),
        kv("Name:           ", display_name(info), palette),
        kv("Status:         ", &info.status, palette),
        kv("Project:        ", project, palette),
        kv("Git Branch:     ", &branch, palette),
        kv("Working Dir:    ", cwd, palette),
        kv("Exe Path:       ", exe, palette),
        kv("Env Vars:       ", &env_str, palette),
        kv("Run Time:       ", &runtime_str, palette),
    ];

    let block = Block::default()
        .title(" Info ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

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
    graph_style: GraphStyle,
    palette: &Palette,
) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(12), // Info table
            Constraint::Fill(1),    // CPU chart (splits remaining space with memory)
            Constraint::Fill(1),    // Memory chart
            Constraint::Length(4),  // Command
        ])
        .split(area);

    render_header(f, sections[0], info, palette);
    render_info_table(f, sections[1], info, palette);

    // --- CPU chart -----------------------------------------------------------
    let cpu_points: Vec<(f64, f64)> = cpu_history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v as f64))
        .collect();
    // Observed max, padded by 15% and floored at 1% so the chart doesn't
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
        palette.claude_style(),
        graph_style,
        palette,
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
        palette.codex_style(),
        graph_style,
        palette,
    );

    render_command(f, sections[4], info, palette);
}

/// Render a compact header block showing the process name, PID, and status.
fn render_header(f: &mut Frame, area: Rect, info: &ProcessInfo, palette: &Palette) {
    let title = format!(" {} — PID {} — {} ", display_name(info), info.pid, info.status);
    let block = Block::default()
        .title(title)
        .title_style(palette.title_style().add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(palette.border_style());
    f.render_widget(block, area);
}

/// Render the full command line in a wrapped paragraph block.
fn render_command(f: &mut Frame, area: Rect, info: &ProcessInfo, palette: &Palette) {
    let cmd_text = info.cmd.join(" ");
    let block = Block::default()
        .title(" Command ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(
        Paragraph::new(cmd_text)
            .block(block)
            .style(Style::new().fg(Color::White))
            .wrap(Wrap { trim: false }),
        area,
    );
}
