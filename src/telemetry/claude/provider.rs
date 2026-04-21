//! [`ClaudeTelemetryProvider`] — the main implementation of [`TelemetryProvider`]
//! for Claude Code sessions.
//!
//! # Overview
//!
//! On each call to [`enrich`](crate::telemetry::TelemetryProvider::enrich) the
//! provider:
//!
//! 1. Filters `processes` to PIDs that have a `~/.claude/sessions/{pid}.json`
//!    entry (using the cached [`SessionIndex`]).
//! 2. For each matched PID, locates the transcript JSONL at
//!    `~/.claude/projects/{cwd-slug}/{session_id}.jsonl`.
//! 3. Tails the transcript (only new bytes) and updates
//!    [`TranscriptAggregates`].
//! 4. Derives [`AgentTelemetry`] from the aggregates and writes it into `out`.
//! 5. Prunes state for PIDs that are no longer running.
//!
//! # Error tolerance
//!
//! The provider never panics. Any I/O or parse failure degrades gracefully:
//! - Missing session file → no entry written for that PID.
//! - Missing transcript file → empty/partial telemetry.
//! - Malformed JSONL lines → skipped (see `parser.rs`).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::process::ProcessInfo;
use crate::telemetry::types::{
    AgentStatus, AgentTelemetry, RecentMessage, TelemetryMap, TelemetryProvider,
};

use super::discovery::{cwd_to_slug, SessionIndex};
use super::health::decay_score;
use super::parser::{TranscriptAggregates, TranscriptTail};
use super::pricing::lookup as lookup_pricing;
use super::subagents;

/// CPU usage percentage above which a session is classified as `Processing`.
const PROCESSING_CPU_THRESHOLD: f32 = 5.0;

/// Seconds of inactivity after an `end_turn` before classifying as `Idle`.
const IDLE_THRESHOLD_SECS: u64 = 10 * 60;

/// Per-session state retained between ticks.
#[derive(Debug, Default)]
struct SessionState {
    /// Incremental JSONL reader.
    tail: TranscriptTail,
    /// Accumulated telemetry from the transcript.
    agg: TranscriptAggregates,
}

/// A [`TelemetryProvider`] that reads Claude Code session files and transcripts.
///
/// Construct via [`ClaudeTelemetryProvider::new`] and register it in the
/// [`TelemetryPipeline`](crate::telemetry::TelemetryPipeline).
pub struct ClaudeTelemetryProvider {
    /// Session metadata index (cached, TTL-based).
    session_index: SessionIndex,
    /// Per-PID incremental reader + accumulator state.
    session_state: HashMap<u32, SessionState>,
    /// Root of `~/.claude/projects/` for transcript path construction.
    projects_dir: Option<PathBuf>,
}

impl ClaudeTelemetryProvider {
    /// Create a new provider, resolving paths relative to the home directory.
    pub fn new() -> Self {
        let projects_dir = dirs::home_dir().map(|h| h.join(".claude").join("projects"));
        Self {
            session_index: SessionIndex::new(),
            session_state: HashMap::new(),
            projects_dir,
        }
    }

    /// Create a provider pointing at custom sessions and projects directories.
    ///
    /// Used in integration tests to inject a synthetic `~/.claude/` tree.
    pub fn from_dirs(sessions_dir: PathBuf, projects_dir: PathBuf) -> Self {
        let mut session_index = SessionIndex::new();
        // Override the sessions directory.
        session_index.set_sessions_dir(sessions_dir);
        Self {
            session_index,
            session_state: HashMap::new(),
            projects_dir: Some(projects_dir),
        }
    }

    /// Build the transcript path for a session.
    fn transcript_path(&self, cwd_slug: &str, session_id: &str) -> Option<PathBuf> {
        let base = self.projects_dir.as_ref()?;
        Some(base.join(cwd_slug).join(format!("{session_id}.jsonl")))
    }

    /// Compute [`AgentStatus`] from aggregates and the process's CPU usage.
    fn infer_status(agg: &TranscriptAggregates, cpu_usage: f32) -> AgentStatus {
        // Priority 1: active CPU means actively processing.
        if cpu_usage > PROCESSING_CPU_THRESHOLD {
            return AgentStatus::Processing;
        }

        // Priority 2: pending tool awaiting result.
        if agg.pending_tool.is_some() {
            return AgentStatus::NeedsInput;
        }

        // Priority 3: ended turn, check recency.
        if agg.last_stop_reason.as_deref() == Some("end_turn") {
            let idle = agg
                .last_event_timestamp
                .as_deref()
                .and_then(parse_timestamp_secs)
                .map(|ts| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    now.saturating_sub(ts)
                })
                .unwrap_or(0);

            if idle > IDLE_THRESHOLD_SECS {
                return AgentStatus::Idle;
            }
            return AgentStatus::WaitingInput;
        }

        AgentStatus::Unknown
    }
}

impl Default for ClaudeTelemetryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryProvider for ClaudeTelemetryProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn enrich(&mut self, processes: &[ProcessInfo], out: &mut TelemetryMap) {
        let live_pids: Vec<u32> = processes.iter().map(|p| p.pid).collect();

        // Prune dead sessions to prevent unbounded memory growth.
        self.session_state.retain(|pid, _| live_pids.contains(pid));
        self.session_index.prune(&live_pids);

        for proc in processes {
            let pid = proc.pid;

            // Resolve session metadata (or skip if no Claude session for this PID).
            // We clone the strings out of the index so the borrow ends before
            // we call `transcript_path`, which borrows `self` immutably.
            let (session_id, cwd_slug) = {
                match self.session_index.get(pid) {
                    Some(meta) => {
                        let slug = cwd_to_slug(&meta.cwd);
                        (meta.session_id.clone(), slug)
                    }
                    None => continue,
                }
            };
            let transcript_path = self.transcript_path(&cwd_slug, &session_id);

            // Get or create per-session state and tail transcript.
            let state = self.session_state.entry(pid).or_default();

            if let Some(ref path) = transcript_path {
                if path.exists() {
                    state.tail.ingest(path, &mut state.agg);
                }
            }

            let agg = &state.agg;

            // Look up pricing for cost and context window.
            let pricing = agg
                .model
                .as_deref()
                .map(lookup_pricing)
                .unwrap_or_else(|| lookup_pricing(""));

            // Subagent count.
            let subagent_count = subagents::count(&cwd_slug, &session_id);

            // Compute decay score.
            let decay = decay_score(agg, Some(pricing.context_window));

            // Infer status.
            let status = Self::infer_status(agg, proc.cpu_usage);

            let telemetry = AgentTelemetry {
                session_id: if session_id.is_empty() {
                    None
                } else {
                    Some(session_id.clone())
                },
                session_name: None, // Not available in session metadata.
                model: agg.model.clone(),
                input_tokens: if agg.input_tokens > 0 || agg.model.is_some() {
                    Some(agg.input_tokens)
                } else {
                    None
                },
                output_tokens: if agg.output_tokens > 0 || agg.model.is_some() {
                    Some(agg.output_tokens)
                } else {
                    None
                },
                cache_read_tokens: if agg.cache_read_tokens > 0 || agg.model.is_some() {
                    Some(agg.cache_read_tokens)
                } else {
                    None
                },
                cache_write_tokens: if agg.cache_write_tokens > 0 || agg.model.is_some() {
                    Some(agg.cache_write_tokens)
                } else {
                    None
                },
                context_tokens: agg.context_tokens,
                context_window: if agg.model.is_some() {
                    Some(pricing.context_window)
                } else {
                    None
                },
                cost_usd: if agg.model.is_some() {
                    Some(agg.cost_usd)
                } else {
                    None
                },
                cost_is_estimated: agg.model.is_some(),
                pending_tool: agg.pending_tool.clone(),
                last_stop_reason: agg.last_stop_reason.clone(),
                subagent_count,
                status: Some(status),
                decay_score: Some(decay),
                recent_messages: agg
                    .recent_messages
                    .iter()
                    .map(|m| RecentMessage {
                        timestamp: m.timestamp.clone(),
                        model: m.model.clone(),
                        stop_reason: m.stop_reason.clone(),
                        preview: m.preview.clone(),
                    })
                    .collect(),
            };

            out.insert(pid, telemetry);
        }
    }
}

/// Parse an RFC 3339 timestamp string to Unix seconds.
///
/// Returns `None` on parse failure.
fn parse_timestamp_secs(ts: &str) -> Option<u64> {
    let parsed =
        time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339).ok()?;
    let unix_secs = parsed.unix_timestamp();
    if unix_secs >= 0 {
        Some(unix_secs as u64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::types::AgentStatus;

    fn make_proc(pid: u32, cpu: f32) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: None,
            name: "claude".to_string(),
            cmd: vec!["claude".to_string()],
            exe_path: None,
            cwd: None,
            cpu_usage: cpu,
            memory_bytes: 0,
            status: "Run".to_string(),
            environ_count: 0,
            start_time: 0,
            run_time: 0,
        }
    }

    #[test]
    fn status_processing_when_high_cpu() {
        let agg = TranscriptAggregates::default();
        assert_eq!(
            ClaudeTelemetryProvider::infer_status(&agg, 10.0),
            AgentStatus::Processing
        );
    }

    #[test]
    fn status_needs_input_when_pending_tool_and_low_cpu() {
        let agg = TranscriptAggregates {
            pending_tool: Some("Bash".to_string()),
            ..Default::default()
        };
        assert_eq!(
            ClaudeTelemetryProvider::infer_status(&agg, 0.0),
            AgentStatus::NeedsInput
        );
    }

    #[test]
    fn status_waiting_input_when_end_turn_recent() {
        // A recent end_turn timestamp should be WaitingInput.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = time::OffsetDateTime::from_unix_timestamp(now_secs as i64)
            .unwrap()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        let agg = TranscriptAggregates {
            last_stop_reason: Some("end_turn".to_string()),
            last_event_timestamp: Some(ts),
            ..Default::default()
        };
        assert_eq!(
            ClaudeTelemetryProvider::infer_status(&agg, 0.0),
            AgentStatus::WaitingInput
        );
    }

    #[test]
    fn status_idle_when_end_turn_old() {
        // A very old end_turn timestamp (Unix epoch) should be Idle.
        let ts = "1970-01-01T00:00:00Z".to_string();
        let agg = TranscriptAggregates {
            last_stop_reason: Some("end_turn".to_string()),
            last_event_timestamp: Some(ts),
            ..Default::default()
        };
        assert_eq!(
            ClaudeTelemetryProvider::infer_status(&agg, 0.0),
            AgentStatus::Idle
        );
    }

    #[test]
    fn status_unknown_when_no_signals() {
        let agg = TranscriptAggregates::default();
        assert_eq!(
            ClaudeTelemetryProvider::infer_status(&agg, 0.0),
            AgentStatus::Unknown
        );
    }

    #[test]
    fn provider_skips_non_claude_pids() {
        let mut provider = ClaudeTelemetryProvider::new();
        let processes = vec![make_proc(99999, 0.0)];
        let mut out = TelemetryMap::new();
        provider.enrich(&processes, &mut out);
        // No session file for PID 99999, so nothing should be written.
        assert!(out.is_empty());
    }
}
