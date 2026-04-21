//! Plain-text table output for `--list` mode.
//!
//! Formats the visible process forest as a fixed-width table suitable for
//! human inspection. When stdout is a TTY the table adapts to the terminal
//! width; when piped it falls back to 120 columns and omits ANSI escape codes.

use std::io::{IsTerminal, Write};

use crate::process::tree::{FlatEntry, ProcessNode};
use crate::process::{display_name, flatten_visible, ActivityState};
use crate::ui::{format_duration_compact, format_memory};

/// Column widths for the fixed-width table.
const PID_W: usize = 8;
const CPU_W: usize = 8;
const MEM_W: usize = 10;
const STATUS_W: usize = 10;
const UPTIME_W: usize = 12;
/// Minimum width allocated for the Name column.
const NAME_MIN_W: usize = 20;
/// Default terminal width when stdout is not a TTY.
const DEFAULT_WIDTH: usize = 120;

/// Print a plain-text process table derived from `forest` to `out`.
///
/// The Name column uses box-drawing tree connectors that match the TUI tree
/// view, so the output feels familiar. For root nodes the activity state is
/// shown as a one-character badge (`●`/`○`/`?`).
///
/// # Arguments
///
/// * `forest` - Root process nodes, already built and sorted.
/// * `out`    - The writer to print to; typically `std::io::stdout()`.
pub fn print_table(forest: &[ProcessNode], out: &mut impl Write) -> std::io::Result<()> {
    let flat = flatten_visible(forest);
    let term_width = terminal_width();

    // Name column gets whatever space remains after fixed columns.
    let fixed = PID_W + 1 + CPU_W + 1 + MEM_W + 1 + STATUS_W + 1 + UPTIME_W + 1;
    let name_w = term_width.saturating_sub(fixed).max(NAME_MIN_W);

    // Header
    writeln!(
        out,
        "{:<pid$} {:<name$} {:>cpu$} {:>mem$} {:<st$} {:<up$}",
        "PID",
        "NAME",
        "CPU%",
        "MEM",
        "STATUS",
        "UPTIME",
        pid = PID_W,
        name = name_w,
        cpu = CPU_W,
        mem = MEM_W,
        st = STATUS_W,
        up = UPTIME_W,
    )?;

    // Separator
    writeln!(
        out,
        "{} {} {} {} {} {}",
        "-".repeat(PID_W),
        "-".repeat(name_w),
        "-".repeat(CPU_W),
        "-".repeat(MEM_W),
        "-".repeat(STATUS_W),
        "-".repeat(UPTIME_W),
    )?;

    for entry in &flat {
        write_row(entry, name_w, out)?;
    }

    Ok(())
}

/// Write a single table row for `entry`.
fn write_row(entry: &FlatEntry, name_w: usize, out: &mut impl Write) -> std::io::Result<()> {
    let name_str = build_name_cell(entry, name_w);
    let cpu = format!("{:.1}%", entry.info.cpu_usage);
    let mem = format_memory(entry.info.memory_bytes);
    let uptime = format_duration_compact(entry.info.run_time);

    writeln!(
        out,
        "{:<pid$} {:<name$} {:>cpu$} {:>mem$} {:<st$} {:<up$}",
        entry.info.pid,
        name_str,
        cpu,
        mem,
        entry.info.status,
        uptime,
        pid = PID_W,
        name = name_w,
        cpu = CPU_W,
        mem = MEM_W,
        st = STATUS_W,
        up = UPTIME_W,
    )
}

/// Build the Name column string: activity badge + tree connector + name.
///
/// The connector uses box-drawing characters to match the TUI tree view.
/// The result is truncated with an ellipsis (`...`) if it exceeds `max_w`.
fn build_name_cell(entry: &FlatEntry, max_w: usize) -> String {
    // Activity badge for root nodes (3 chars including trailing space, or "").
    let badge = if entry.is_root {
        match entry.activity {
            Some(ActivityState::Active) => "\u{25cf} ", // ●
            Some(ActivityState::Idle) => "\u{25cb} ",   // ○
            Some(ActivityState::Unknown) | None => "? ",
        }
    } else {
        ""
    };

    // Box-drawing tree connector (mirrors tree_view.rs logic).
    let mut connector = String::new();
    for _ in 0..entry.depth.saturating_sub(1) {
        connector.push_str("  ");
    }
    if entry.depth > 0 {
        if entry.is_last_sibling {
            connector.push_str("\u{2514}\u{2500} "); // └─
        } else {
            connector.push_str("\u{251c}\u{2500} "); // ├─
        }
    }

    // Expand indicator for nodes with children.
    let expand = if entry.has_children {
        if entry.expanded {
            "\u{25bc} " // ▼
        } else {
            "\u{25b6} " // ▶
        }
    } else {
        ""
    };

    let name = display_name(&entry.info);
    let full = format!("{badge}{connector}{expand}{name}");

    // Truncate visually if it exceeds the column width. We do a naive
    // char-count truncation — multi-width CJK chars are not handled, but
    // process names are always ASCII in practice.
    if full.chars().count() > max_w && max_w > 3 {
        let mut s: String = full.chars().take(max_w - 3).collect();
        s.push_str("...");
        s
    } else {
        full
    }
}

/// Return the terminal width, or the default fallback when stdout is not a TTY.
fn terminal_width() -> usize {
    if std::io::stdout().is_terminal() {
        // crossterm can query the terminal size. Fall back to default on error.
        crossterm::terminal::size()
            .map(|(cols, _)| cols as usize)
            .unwrap_or(DEFAULT_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{build_forest, ProcessInfo};

    fn make_proc(pid: u32, parent: Option<u32>, name: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: parent,
            name: name.to_string(),
            cmd: vec![name.to_string()],
            exe_path: None,
            cwd: None,
            cpu_usage: 1.5,
            memory_bytes: 1_048_576,
            status: "Run".to_string(),
            environ_count: 0,
            start_time: 0,
            run_time: 90,
        }
    }

    #[test]
    fn table_contains_header_and_pid() {
        let procs = vec![make_proc(1, None, "claude"), make_proc(2, Some(1), "node")];
        let forest = build_forest(&procs);
        let mut buf = Vec::new();
        print_table(&forest, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("PID"), "header missing PID column");
        assert!(out.contains("NAME"), "header missing NAME column");
        assert!(out.contains("CPU%"), "header missing CPU% column");
        // Both rows should appear.
        assert!(out.contains('1'), "PID 1 missing");
        assert!(out.contains('2'), "PID 2 missing");
    }

    #[test]
    fn name_cell_truncates_long_names() {
        let long_name = "a".repeat(100);
        let entry = FlatEntry {
            info: ProcessInfo {
                pid: 1,
                parent_pid: None,
                name: long_name.clone(),
                cmd: vec![long_name],
                exe_path: None,
                cwd: None,
                cpu_usage: 0.0,
                memory_bytes: 0,
                status: "Run".to_string(),
                environ_count: 0,
                start_time: 0,
                run_time: 0,
            },
            depth: 0,
            is_root: false,
            expanded: false,
            has_children: false,
            is_last_sibling: true,
            kind: None,
            subtree_stats: Default::default(),
            activity: None,
            activity_since: None,
        };
        let cell = build_name_cell(&entry, 20);
        assert!(
            cell.chars().count() <= 20,
            "cell should be at most 20 chars, got {}",
            cell.chars().count()
        );
        assert!(cell.ends_with("..."), "truncated cell should end with ...");
    }
}
