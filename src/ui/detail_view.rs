use std::path::Path;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, RenderDirection, Sparkline, Wrap},
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

    // Render right-to-left so the most recent sample is anchored to the right
    // edge and history scrolls off to the left as it ages.
    let sparkline = Sparkline::default()
        .block(block)
        .data(data)
        .style(style)
        .direction(RenderDirection::RightToLeft);
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

    // Multiply CPU float (0–100) by 10 to preserve one decimal of precision as u64.
    let cpu_data: Vec<u64> = cpu_history.iter().map(|&v| (v * 10.0) as u64).collect();
    let cpu_title = format!(" CPU Usage — {:.1}% ", info.cpu_usage);
    render_sparkline(f, sections[2], &cpu_title, &cpu_data, CLAUDE_SPARKLINE_STYLE);

    // Convert bytes to MB for a readable scale in the sparkline.
    let mem_data: Vec<u64> = mem_history.iter().map(|&b| b / (1024 * 1024)).collect();
    let mem_title = format!(" Memory Usage — {} ", format_memory(info.memory_bytes));
    render_sparkline(
        f,
        sections[3],
        &mem_title,
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
