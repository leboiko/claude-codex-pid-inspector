use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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
use crate::process::SubtreeStats;
use crate::telemetry::AgentTelemetry;

use super::format::{format_duration_full, format_memory};
use super::styles::{GraphStyle, Palette};

// No constants — all styles now come from Palette.

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
        .style(palette.base_style())
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
        Span::styled(val, Style::new().fg(palette.foreground)),
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
///
/// When `subtree_stats` is `Some`, three additional rows are appended showing
/// the aggregated CPU, memory, and child-process count for the entire subtree.
fn render_info_table(
    f: &mut Frame,
    area: Rect,
    info: &ProcessInfo,
    subtree_stats: Option<SubtreeStats>,
    palette: &Palette,
) {
    // Build all formatted values as owned Strings first, then borrow them
    // as &str for `kv`. This avoids passing temporaries by value into the
    // Span, which requires `'static` lifetimes in ratatui's Cow-based API.
    let parent = info
        .parent_pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| "—".into());
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

    // Pre-compute optional subtree strings so they live long enough for the
    // `kv` borrows, which must survive until `Paragraph::new(lines)` below.
    // Rust's borrow checker requires these to be declared in the same scope as
    // `lines`, not inside a nested `if let` block that ends before their use.
    let subtree_cpu_str = subtree_stats.map(|s| format!("{:.1}%", s.total_cpu));
    let subtree_mem_str = subtree_stats.map(|s| format_memory(s.total_memory));
    let child_count_str = subtree_stats.map(|s| (s.process_count.saturating_sub(1)).to_string());

    let mut lines = vec![
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

    // Append subtree aggregate rows when stats are available.
    if let (Some(ref cpu), Some(ref mem), Some(ref count)) =
        (&subtree_cpu_str, &subtree_mem_str, &child_count_str)
    {
        lines.push(kv("CPU (subtree):  ", cpu, palette));
        lines.push(kv("Mem (subtree):  ", mem, palette));
        lines.push(kv("Child procs:    ", count, palette));
    }

    let block = Block::default()
        .title(" Info ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Render the full detail view for a selected process.
///
/// When `telemetry` is `Some` and `has_data()` is true, extra Claude-specific
/// sections are appended below the command block:
/// - Token statistics
/// - Health / decay score bar
/// - Pending tool indicator
/// - Recent messages (last 5 assistant turns)
/// - Subagent count summary
///
/// Non-Claude processes (Codex roots, plain children) receive the original
/// layout unchanged.
///
/// # Arguments
///
/// * `f`              - Ratatui frame.
/// * `area`           - Available screen area.
/// * `info`           - Process information to display.
/// * `cpu_history`    - Recent CPU usage samples (0.0–100.0).
/// * `mem_history`    - Recent memory usage samples in bytes.
/// * `subtree_stats`  - Optional aggregate stats for the process subtree; when
///   `Some`, three extra rows appear in the info table.
/// * `graph_style`    - Dot vs bar chart style.
/// * `palette`        - Active color theme.
/// * `telemetry`      - Optional Claude session telemetry; `None` for non-Claude processes.
#[allow(clippy::too_many_arguments)]
pub fn render_detail_view(
    f: &mut Frame,
    area: Rect,
    info: &ProcessInfo,
    cpu_history: &[f32],
    mem_history: &[u64],
    subtree_stats: Option<SubtreeStats>,
    graph_style: GraphStyle,
    palette: &Palette,
    telemetry: Option<&AgentTelemetry>,
) {
    // Show Claude-specific sections only when there is real telemetry data.
    let show_claude = telemetry.is_some_and(|t| t.has_data());

    // The info table grows by 3 rows when subtree stats are present.
    let info_height = if subtree_stats.is_some() { 15 } else { 12 };

    let mut constraints = vec![
        Constraint::Length(3),           // Header
        Constraint::Length(info_height), // Info table
        Constraint::Fill(1),             // CPU chart
        Constraint::Fill(1),             // Memory chart
        Constraint::Length(4),           // Command
    ];

    if show_claude {
        constraints.push(Constraint::Length(6)); // Token stats
        constraints.push(Constraint::Length(3)); // Health/decay score
        constraints.push(Constraint::Length(3)); // Pending tool
        constraints.push(Constraint::Min(10)); // Recent messages
        constraints.push(Constraint::Length(3)); // Subagent summary
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_header(f, sections[0], info, palette);
    render_info_table(f, sections[1], info, subtree_stats, palette);

    // --- CPU chart -----------------------------------------------------------
    let cpu_points: Vec<(f64, f64)> = cpu_history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v as f64))
        .collect();
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
    let mem_observed_max =
        mem_history.iter().copied().max().unwrap_or(0) as f64 / (1024.0 * 1024.0);
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

    // --- Claude-specific telemetry sections ----------------------------------
    if show_claude {
        if let Some(tel) = telemetry {
            render_token_stats(f, sections[5], tel, palette);
            render_health_bar(f, sections[6], tel, palette);
            render_pending_tool(f, sections[7], tel, palette);
            render_recent_messages(f, sections[8], tel, palette);
            render_subagent_summary(f, sections[9], tel, palette);
        }
    }
}

/// Render token statistics for the selected Claude session.
fn render_token_stats(f: &mut Frame, area: Rect, tel: &AgentTelemetry, palette: &Palette) {
    let input_str = tel
        .input_tokens
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "\u{2013}".to_string());
    let output_str = tel
        .output_tokens
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "\u{2013}".to_string());
    let cache_r_str = tel
        .cache_read_tokens
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "\u{2013}".to_string());
    let cache_w_str = tel
        .cache_write_tokens
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "\u{2013}".to_string());
    let ctx_str = tel
        .context_tokens
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "\u{2013}".to_string());
    let window_str = tel
        .context_window
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "\u{2013}".to_string());
    let cost_str = tel
        .cost_usd
        .map(|c| format!("${c:.4}"))
        .unwrap_or_else(|| "\u{2013}".to_string());

    let lines = vec![
        kv("Input tokens:   ", &input_str, palette),
        kv("Output tokens:  ", &output_str, palette),
        kv("Cache read:     ", &cache_r_str, palette),
        kv("Cache write:    ", &cache_w_str, palette),
        kv("Context tokens: ", &ctx_str, palette),
        kv("Context window: ", &window_str, palette),
        kv("Cost (est.):    ", &cost_str, palette),
    ];

    let block = Block::default()
        .title(" Token Stats ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Render the decay / health score as a Unicode block bar.
///
/// Format: `Decay  ████████░░░░░░░░░░░░  37`
fn render_health_bar(f: &mut Frame, area: Rect, tel: &AgentTelemetry, palette: &Palette) {
    let score = tel.decay_score.unwrap_or(0);
    let bar_len = 20usize;
    let filled = (score as usize * bar_len / 100).min(bar_len);
    let empty = bar_len - filled;

    let bar: String = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(empty);

    let bar_style = if score < 40 {
        Style::new().fg(Color::Green)
    } else if score < 70 {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::Red)
    };

    let line = Line::from(vec![
        Span::styled("Decay  ", palette.label_style()),
        Span::styled(bar, bar_style),
        Span::styled(format!("  {score}"), Style::new().fg(palette.foreground)),
    ]);

    let block = Block::default()
        .title(" Health ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(Paragraph::new(line).block(block), area);
}

/// Render the pending-tool section.
fn render_pending_tool(f: &mut Frame, area: Rect, tel: &AgentTelemetry, palette: &Palette) {
    let content = match &tel.pending_tool {
        Some(tool) => Line::from(vec![
            Span::styled("Pending: ", palette.label_style()),
            Span::styled(
                tool.clone(),
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ]),
        None => Line::from(vec![Span::styled("No pending tool", palette.dim_style())]),
    };

    let block = Block::default()
        .title(" Pending Tool ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(Paragraph::new(content).block(block), area);
}

/// Render the last 5 assistant messages.
fn render_recent_messages(f: &mut Frame, area: Rect, tel: &AgentTelemetry, palette: &Palette) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let lines: Vec<Line> = tel
        .recent_messages
        .iter()
        .map(|msg| {
            // Compute relative timestamp in MM:SS from the message timestamp.
            let rel_str = parse_ts_to_unix(msg.timestamp.as_str())
                .map(|ts| {
                    let elapsed = now_secs.saturating_sub(ts);
                    let m = elapsed / 60;
                    let s = elapsed % 60;
                    format!("{m:02}:{s:02}")
                })
                .unwrap_or_else(|| "??:??".to_string());

            // Shorten model name.
            let model_short = shorten_model(&msg.model);
            let stop = msg.stop_reason.as_deref().unwrap_or("?");

            // Truncate preview to 3 lines of ~80 chars.
            let preview_lines: Vec<&str> = msg.preview.lines().take(3).collect();
            let preview = preview_lines.join(" ");
            let preview_trunc = super::format::truncate_chars(&preview, 80);

            let header = format!("[{rel_str}] {model_short} ({stop})");
            Line::from(vec![
                Span::styled(header, palette.label_style()),
                Span::raw(" "),
                Span::styled(preview_trunc, Style::new().fg(palette.foreground)),
            ])
        })
        .collect();

    let block = Block::default()
        .title(" Recent Messages ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

/// Render the subagent count summary.
fn render_subagent_summary(f: &mut Frame, area: Rect, tel: &AgentTelemetry, palette: &Palette) {
    let line = Line::from(vec![
        Span::styled("Subagents: ", palette.label_style()),
        Span::styled(
            tel.subagent_count.to_string(),
            Style::new().fg(palette.foreground),
        ),
    ]);

    let block = Block::default()
        .title(" Subagents ")
        .title_style(palette.title_style())
        .borders(Borders::ALL)
        .border_style(palette.border_style());

    f.render_widget(Paragraph::new(line).block(block), area);
}

/// Shorten a model name for display (remove the `claude-` prefix).
fn shorten_model(model: &str) -> String {
    model.strip_prefix("claude-").unwrap_or(model).to_string()
}

/// Parse an RFC 3339 timestamp string to Unix seconds using the `time` crate.
fn parse_ts_to_unix(ts: &str) -> Option<u64> {
    let dt =
        time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339).ok()?;
    let secs = dt.unix_timestamp();
    if secs >= 0 {
        Some(secs as u64)
    } else {
        None
    }
}

/// Render a compact header block showing the process name, PID, and status.
fn render_header(f: &mut Frame, area: Rect, info: &ProcessInfo, palette: &Palette) {
    let title = format!(
        " {} — PID {} — {} ",
        display_name(info),
        info.pid,
        info.status
    );
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
            .style(Style::new().fg(palette.foreground))
            .wrap(Wrap { trim: false }),
        area,
    );
}
