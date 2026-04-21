//! JSON snapshot output for `--json` mode.
//!
//! Serialises a full process-tree snapshot to a stable, versioned JSON schema.
//! See `docs/output-schema.md` for the field reference and stability promise.

use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::app::AgentSummary;
use crate::process::scanner::SystemStats;
use crate::process::tree::ProcessNode;
use crate::process::{display_name, process_kind, ActivityState, ProcessKind};

/// Schema version embedded in every snapshot.
///
/// Bump to 2 only for removals or renames; additive changes stay at 1.
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Top-level snapshot
// ---------------------------------------------------------------------------

/// A complete process-tree snapshot suitable for JSON serialisation.
///
/// `schema_version = 1` will never remove or rename fields. Additive changes
/// (new optional fields) are a minor bump only. Breaking changes bump to 2.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    /// Monotonically increasing schema version.
    pub schema_version: u32,
    /// RFC 3339 UTC timestamp when this snapshot was taken.
    pub generated_at: String,
    /// Version string of the `agentop` binary that produced this snapshot.
    pub agentop_version: String,
    /// System-wide resource usage at snapshot time.
    pub system: SystemInfo,
    /// Aggregated counts and resource usage for all detected agent processes.
    pub agent_summary: AgentSummaryJson,
    /// Root-level agent processes; children are nested inside each entry.
    pub processes: Vec<ProcessEntry>,
}

/// System-wide resource metrics.
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    /// Global CPU usage percent across all cores.
    pub cpu_usage_percent: f32,
    /// Total physical memory installed (bytes).
    pub total_memory_bytes: u64,
    /// Currently used physical memory (bytes).
    pub used_memory_bytes: u64,
    /// Total swap space (bytes).
    pub total_swap_bytes: u64,
    /// Currently used swap space (bytes).
    pub used_swap_bytes: u64,
    /// Number of logical CPU cores.
    pub cpu_count: usize,
}

/// Aggregated counts and resource usage for all detected agent processes.
#[derive(Debug, Serialize)]
pub struct AgentSummaryJson {
    /// Number of Claude Code root processes currently running.
    pub claude_count: usize,
    /// Number of Codex CLI root processes currently running.
    pub codex_count: usize,
    /// Total CPU usage (%) across all agent subtrees.
    pub total_cpu_percent: f32,
    /// Total resident memory (bytes) across all agent subtrees.
    pub total_memory_bytes: u64,
}

/// A single process entry in the tree, with nested children.
#[derive(Debug, Serialize)]
pub struct ProcessEntry {
    /// Numeric process identifier.
    pub pid: u32,
    /// PID of the parent process; `null` for root entries.
    pub parent_pid: Option<u32>,
    /// Process kind: `"claude"`, `"codex"`, or `null` for plain children.
    pub kind: Option<String>,
    /// Short OS process name.
    pub name: String,
    /// Friendly display name (e.g. `"claude"` even when OS name is `"node"`).
    pub display_name: String,
    /// Full command line split into argv tokens.
    pub cmd: Vec<String>,
    /// Absolute path to the executable, or `null` if unavailable.
    pub exe_path: Option<String>,
    /// Working directory of the process, or `null` if unavailable.
    pub cwd: Option<String>,
    /// CPU usage percent (3-sample rolling average).
    pub cpu_percent: f32,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
    /// Human-readable process status string (e.g. `"Run"`, `"Sleep"`).
    pub status: String,
    /// Unix timestamp (seconds since epoch) when the process started.
    pub start_time_unix: u64,
    /// Seconds the process has been running.
    pub uptime_seconds: u64,
    /// Activity classification for root agent processes; `null` for non-roots.
    pub activity_state: Option<String>,
    /// Seconds the process has been in the current activity state; `null` for non-roots.
    pub activity_state_seconds: Option<u64>,
    /// Direct child processes (same shape, recursive).
    pub children: Vec<ProcessEntry>,
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// Build a [`Snapshot`] from the current process forest and system stats.
///
/// # Arguments
///
/// * `forest`          - Root process nodes with fully computed subtree stats.
/// * `stats`           - System-wide resource snapshot.
/// * `summary`         - Pre-computed agent summary.
/// * `activity_states` - Per-PID activity state injected by `App`.
/// * `activity_since`  - Per-PID time-in-state in seconds.
pub fn build_snapshot(
    forest: &[ProcessNode],
    stats: &SystemStats,
    summary: &AgentSummary,
    activity_states: &std::collections::HashMap<u32, ActivityState>,
    activity_since_secs: &std::collections::HashMap<u32, u64>,
) -> Snapshot {
    let now = OffsetDateTime::now_utc();
    let generated_at = now
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::from("unknown"));

    let processes: Vec<ProcessEntry> = forest
        .iter()
        .map(|node| build_entry(node, activity_states, activity_since_secs))
        .collect();

    Snapshot {
        schema_version: SCHEMA_VERSION,
        generated_at,
        agentop_version: env!("CARGO_PKG_VERSION").to_string(),
        system: SystemInfo {
            cpu_usage_percent: stats.cpu_usage,
            total_memory_bytes: stats.total_memory,
            used_memory_bytes: stats.used_memory,
            total_swap_bytes: stats.total_swap,
            used_swap_bytes: stats.used_swap,
            cpu_count: stats.cpu_count,
        },
        agent_summary: AgentSummaryJson {
            claude_count: summary.claude_count,
            codex_count: summary.codex_count,
            total_cpu_percent: summary.total_cpu,
            total_memory_bytes: summary.total_memory,
        },
        processes,
    }
}

/// Build a single [`ProcessEntry`] from a [`ProcessNode`], recursing into children.
fn build_entry(
    node: &ProcessNode,
    activity_states: &std::collections::HashMap<u32, ActivityState>,
    activity_since_secs: &std::collections::HashMap<u32, u64>,
) -> ProcessEntry {
    let info = &node.info;
    let pid = info.pid;

    let kind_str = match process_kind(info) {
        Some(ProcessKind::Claude) => Some("claude".to_string()),
        Some(ProcessKind::Codex) => Some("codex".to_string()),
        None => None,
    };

    // Activity state and time-in-state are only meaningful for root nodes.
    let (activity_state, activity_state_seconds) = if node.is_root {
        let state_str = activity_states.get(&pid).map(|s| match s {
            ActivityState::Active => "active".to_string(),
            ActivityState::Idle => "idle".to_string(),
            ActivityState::Unknown => "unknown".to_string(),
        });
        // Default to null if we have no timing data yet.
        let secs = activity_since_secs.get(&pid).copied();
        (state_str, secs)
    } else {
        (None, None)
    };

    let children = node
        .children
        .iter()
        .map(|child| build_entry(child, activity_states, activity_since_secs))
        .collect();

    ProcessEntry {
        pid,
        parent_pid: info.parent_pid,
        kind: kind_str,
        name: info.name.clone(),
        display_name: display_name(info).to_string(),
        cmd: info.cmd.clone(),
        exe_path: info.exe_path.clone(),
        cwd: info.cwd.clone(),
        cpu_percent: info.cpu_usage,
        memory_bytes: info.memory_bytes,
        status: info.status.clone(),
        start_time_unix: info.start_time,
        uptime_seconds: info.run_time,
        activity_state,
        activity_state_seconds,
        children,
    }
}

/// Serialise a snapshot to a compact (single-line) JSON string.
///
/// # Errors
///
/// Returns an error if serde_json serialisation fails (which should not happen
/// for this well-typed structure).
pub fn to_compact_json(snapshot: &Snapshot) -> serde_json::Result<String> {
    serde_json::to_string(snapshot)
}

/// Serialise a snapshot to a pretty-printed (indented) JSON string.
///
/// # Errors
///
/// Returns an error if serde_json serialisation fails.
pub fn to_pretty_json(snapshot: &Snapshot) -> serde_json::Result<String> {
    serde_json::to_string_pretty(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AgentSummary;
    use crate::process::scanner::SystemStats;
    use crate::process::{build_forest, ProcessInfo};
    use std::collections::HashMap;

    fn make_proc(pid: u32, parent: Option<u32>, name: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: parent,
            name: name.to_string(),
            cmd: vec![name.to_string()],
            exe_path: None,
            cwd: None,
            cpu_usage: 2.5,
            memory_bytes: 1_048_576,
            status: "Run".to_string(),
            environ_count: 0,
            start_time: 1_700_000_000,
            run_time: 300,
        }
    }

    fn default_stats() -> SystemStats {
        SystemStats {
            cpu_usage: 12.5,
            total_memory: 16_000_000_000,
            used_memory: 8_000_000_000,
            total_swap: 4_000_000_000,
            used_swap: 0,
            cpu_count: 8,
        }
    }

    fn default_summary() -> AgentSummary {
        AgentSummary {
            claude_count: 1,
            codex_count: 0,
            total_cpu: 2.5,
            total_memory: 1_048_576,
        }
    }

    #[test]
    fn snapshot_schema_version_is_one() {
        let procs = vec![make_proc(1, None, "claude")];
        let forest = build_forest(&procs);
        let snap = build_snapshot(
            &forest,
            &default_stats(),
            &default_summary(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(snap.schema_version, 1);
    }

    #[test]
    fn compact_json_is_single_line() {
        let procs = vec![make_proc(1, None, "claude")];
        let forest = build_forest(&procs);
        let snap = build_snapshot(
            &forest,
            &default_stats(),
            &default_summary(),
            &HashMap::new(),
            &HashMap::new(),
        );
        let json = to_compact_json(&snap).unwrap();
        // Compact JSON must not contain newlines.
        assert!(!json.contains('\n'), "compact JSON must be single-line");
    }

    #[test]
    fn pretty_json_has_indentation() {
        let procs = vec![make_proc(1, None, "claude")];
        let forest = build_forest(&procs);
        let snap = build_snapshot(
            &forest,
            &default_stats(),
            &default_summary(),
            &HashMap::new(),
            &HashMap::new(),
        );
        let json = to_pretty_json(&snap).unwrap();
        assert!(json.contains('\n'), "pretty JSON should be multi-line");
        assert!(json.contains("  "), "pretty JSON should be indented");
    }

    #[test]
    fn snapshot_contains_correct_pid_and_kind() {
        let procs = vec![
            make_proc(10, None, "claude"),
            make_proc(11, Some(10), "node"),
        ];
        let forest = build_forest(&procs);
        let snap = build_snapshot(
            &forest,
            &default_stats(),
            &default_summary(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(snap.processes.len(), 1);
        let root = &snap.processes[0];
        assert_eq!(root.pid, 10);
        assert_eq!(root.kind.as_deref(), Some("claude"));
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].pid, 11);
        assert!(root.children[0].kind.is_none());
    }

    #[test]
    fn activity_state_present_for_root_absent_for_child() {
        let procs = vec![
            make_proc(10, None, "claude"),
            make_proc(11, Some(10), "node"),
        ];
        let forest = build_forest(&procs);

        let mut activity = HashMap::new();
        activity.insert(10u32, ActivityState::Active);
        let mut secs = HashMap::new();
        secs.insert(10u32, 42u64);

        let snap = build_snapshot(
            &forest,
            &default_stats(),
            &default_summary(),
            &activity,
            &secs,
        );
        let root = &snap.processes[0];
        assert_eq!(root.activity_state.as_deref(), Some("active"));
        assert_eq!(root.activity_state_seconds, Some(42));

        let child = &root.children[0];
        assert!(child.activity_state.is_none());
        assert!(child.activity_state_seconds.is_none());
    }

    #[test]
    fn generated_at_is_rfc3339() {
        let forest = build_forest(&[]);
        let snap = build_snapshot(
            &forest,
            &default_stats(),
            &default_summary(),
            &HashMap::new(),
            &HashMap::new(),
        );
        // RFC 3339 timestamps contain 'T' and 'Z' (or offset).
        assert!(
            snap.generated_at.contains('T'),
            "not RFC 3339: {}",
            snap.generated_at
        );
    }
}
