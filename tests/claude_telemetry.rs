//! Integration tests for the Claude telemetry provider.
//!
//! These tests create a synthetic `~/.claude/` tree under a `tempdir` and drive
//! `ClaudeTelemetryProvider::enrich` to verify end-to-end behavior without any
//! real Claude process running.
//!
//! Test organisation:
//! 1. Fixture-based golden tests for JSONL parsing.
//! 2. Provider enrich tests with synthetic session metadata.
//! 3. Incremental tail / truncation tests.

mod common;

use std::io::Write;
use std::path::PathBuf;

use agentop::process::ProcessInfo;
use agentop::telemetry::{AgentStatus, ClaudeTelemetryProvider, TelemetryMap, TelemetryProvider};

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("claude")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("fixture {name} not found: {e}"))
}

// ---------------------------------------------------------------------------
// Fixture golden tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_text_turn_parses_tokens() {
    use agentop::telemetry::claude::parser::{apply_events, TranscriptAggregates};
    use agentop::telemetry::claude::schema::TranscriptEvent;

    let content = read_fixture("assistant_text_turn.jsonl");
    let events: Vec<TranscriptEvent> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse fixture line"))
        .collect();

    let mut agg = TranscriptAggregates::default();
    apply_events(events, &mut agg);

    assert_eq!(agg.input_tokens, 100);
    assert_eq!(agg.output_tokens, 50);
    assert_eq!(agg.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(agg.last_stop_reason.as_deref(), Some("end_turn"));
    assert!(agg.pending_tool.is_none());
    assert_eq!(agg.recent_messages.len(), 1);
}

#[test]
fn fixture_tool_use_pending_sets_pending_tool() {
    use agentop::telemetry::claude::parser::{apply_events, TranscriptAggregates};
    use agentop::telemetry::claude::schema::TranscriptEvent;

    let content = read_fixture("assistant_tool_use_pending.jsonl");
    let events: Vec<TranscriptEvent> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse fixture line"))
        .collect();

    let mut agg = TranscriptAggregates::default();
    apply_events(events, &mut agg);

    assert_eq!(agg.pending_tool.as_deref(), Some("Bash"));
    assert_eq!(agg.last_stop_reason.as_deref(), Some("tool_use"));
    // Two assistant events: input = 100 + 200 = 300.
    assert_eq!(agg.input_tokens, 300);
}

#[test]
fn fixture_tool_use_resolved_clears_pending() {
    use agentop::telemetry::claude::parser::{apply_events, TranscriptAggregates};
    use agentop::telemetry::claude::schema::TranscriptEvent;

    let content = read_fixture("assistant_tool_use_resolved.jsonl");
    let events: Vec<TranscriptEvent> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse fixture line"))
        .collect();

    let mut agg = TranscriptAggregates::default();
    apply_events(events, &mut agg);

    assert!(
        agg.pending_tool.is_none(),
        "tool was resolved; pending_tool should be None"
    );
    assert_eq!(agg.last_stop_reason.as_deref(), Some("end_turn"));
    // Two assistant events: 100 + 150 = 250 input tokens.
    assert_eq!(agg.input_tokens, 250);
}

#[test]
fn fixture_truncated_parses_correctly() {
    use agentop::telemetry::claude::parser::{apply_events, TranscriptAggregates};
    use agentop::telemetry::claude::schema::TranscriptEvent;

    let content = read_fixture("truncated.jsonl");
    let events: Vec<TranscriptEvent> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse fixture line"))
        .collect();

    let mut agg = TranscriptAggregates::default();
    apply_events(events, &mut agg);

    assert_eq!(agg.input_tokens, 42);
}

// ---------------------------------------------------------------------------
// Provider enrich tests with synthetic filesystem
// ---------------------------------------------------------------------------

/// Build a synthetic `~/.claude/` directory tree and return a configured provider.
fn make_provider_with_sessions(
    base_dir: &std::path::Path,
    sessions: &[(u32, &str, &str)], // (pid, session_id, cwd)
    transcripts: &[(/* session_id */ &str, /* cwd_slug */ &str, &str)], // content
) -> ClaudeTelemetryProvider {
    let sessions_dir = base_dir.join("sessions");
    let projects_dir = base_dir.join("projects");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::create_dir_all(&projects_dir).unwrap();

    for (pid, session_id, cwd) in sessions {
        let meta = serde_json::json!({
            "pid": pid,
            "sessionId": session_id,
            "cwd": cwd,
            "startedAt": 1_700_000_000_000u64,
            "version": "2.1.116",
            "peerProtocol": 1,
            "kind": "interactive",
            "entrypoint": "cli"
        });
        std::fs::write(
            sessions_dir.join(format!("{pid}.json")),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
    }

    for (session_id, cwd_slug, content) in transcripts {
        let transcript_dir = projects_dir.join(cwd_slug);
        std::fs::create_dir_all(&transcript_dir).unwrap();
        std::fs::write(transcript_dir.join(format!("{session_id}.jsonl")), content).unwrap();
    }

    // Build provider pointing at our temp directory.
    // We need to use the internal structure — let's build it manually.
    build_provider_at(base_dir)
}

/// Build a `ClaudeTelemetryProvider` that reads from `base_dir` instead of `~/.claude/`.
///
/// This uses a small test shim — we expose a `from_dirs` constructor for tests.
fn build_provider_at(base_dir: &std::path::Path) -> ClaudeTelemetryProvider {
    ClaudeTelemetryProvider::from_dirs(base_dir.join("sessions"), base_dir.join("projects"))
}

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

// ---------------------------------------------------------------------------
// ClaudeTelemetryProvider needs a testable constructor.
// ---------------------------------------------------------------------------

// Add a from_dirs constructor for test use. We'll add it to the provider module.

#[test]
fn provider_enrich_two_sessions_one_empty_one_short() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let text_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-04-21T10:00:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#;

    let sessions = [(42u32, "session-abc", "/Users/test/proj1")];
    let cwd_slug = "-Users-test-proj1";
    let transcripts = [("session-abc", cwd_slug, text_turn)];

    let mut provider = make_provider_with_sessions(base, &sessions, &transcripts);
    let procs = vec![make_proc(42, 0.0)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    assert_eq!(out.len(), 1, "should have one entry for PID 42");
    let tel = out.get(&42).unwrap();
    assert_eq!(tel.session_id.as_deref(), Some("session-abc"));
    assert_eq!(tel.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(tel.input_tokens, Some(100));
    assert_eq!(tel.output_tokens, Some(50));
    assert!(tel.cost_usd.is_some());
    assert!(tel.cost_is_estimated);
}

#[test]
fn provider_enrich_pid_without_session_file_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    std::fs::create_dir_all(base.join("sessions")).unwrap();
    std::fs::create_dir_all(base.join("projects")).unwrap();

    let mut provider = build_provider_at(base);
    let procs = vec![make_proc(99999, 0.0)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);
    assert!(out.is_empty(), "no session file → no telemetry entry");
}

#[test]
fn provider_enrich_missing_transcript_still_writes_session_id() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let sessions_dir = base.join("sessions");
    let projects_dir = base.join("projects");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::create_dir_all(&projects_dir).unwrap();

    // Write session metadata but no transcript.
    let meta = serde_json::json!({
        "pid": 123,
        "sessionId": "no-transcript",
        "cwd": "/a/b",
        "startedAt": 0,
        "version": "2.1.0",
        "peerProtocol": 1,
        "kind": "interactive",
        "entrypoint": "cli"
    });
    std::fs::write(
        sessions_dir.join("123.json"),
        serde_json::to_string(&meta).unwrap(),
    )
    .unwrap();

    let mut provider = build_provider_at(base);
    let procs = vec![make_proc(123, 0.0)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    // Should have an entry with session_id populated but no token data.
    let tel = out.get(&123).unwrap();
    assert_eq!(tel.session_id.as_deref(), Some("no-transcript"));
    assert!(tel.input_tokens.is_none());
}

#[test]
fn provider_incremental_tail_advances_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let text_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-04-21T10:00:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#;

    let sessions = [(42u32, "sess-1", "/Users/me/project")];
    let cwd_slug = "-Users-me-project";
    let transcripts = [("sess-1", cwd_slug, text_turn)];

    let mut provider = make_provider_with_sessions(base, &sessions, &transcripts);
    let procs = vec![make_proc(42, 0.0)];

    // First enrich — reads the existing line.
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);
    assert_eq!(out[&42].input_tokens, Some(100));

    // Append a second identical line.
    let transcript_path = base.join("projects").join(cwd_slug).join("sess-1.jsonl");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&transcript_path)
        .unwrap();
    writeln!(f, "{}", text_turn).unwrap();
    drop(f);

    // Second enrich — should only read the new line.
    let mut out2 = TelemetryMap::new();
    provider.enrich(&procs, &mut out2);
    assert_eq!(
        out2[&42].input_tokens,
        Some(200),
        "should accumulate across ticks"
    );
}

#[test]
fn provider_handles_process_exit_prunes_state() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let text_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-04-21T10:00:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#;

    let sessions = [(55u32, "sess-exit", "/proj")];
    let cwd_slug = "-proj";
    let transcripts = [("sess-exit", cwd_slug, text_turn)];

    let mut provider = make_provider_with_sessions(base, &sessions, &transcripts);

    // First tick — PID 55 is alive.
    let procs = vec![make_proc(55, 0.0)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);
    assert!(out.contains_key(&55));

    // Second tick — PID 55 gone.
    let mut out2 = TelemetryMap::new();
    provider.enrich(&[], &mut out2);
    assert!(!out2.contains_key(&55), "dead PIDs should be pruned");
}

#[test]
fn provider_status_processing_when_high_cpu() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let text_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-04-21T10:00:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#;

    let sessions = [(77u32, "sess-cpu", "/proj2")];
    let cwd_slug = "-proj2";
    let transcripts = [("sess-cpu", cwd_slug, text_turn)];

    let mut provider = make_provider_with_sessions(base, &sessions, &transcripts);
    let procs = vec![make_proc(77, 50.0)]; // high CPU
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    assert_eq!(out[&77].status, Some(AgentStatus::Processing));
}

#[test]
fn provider_decay_score_set() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let text_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-04-21T10:00:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]}}"#;

    let sessions = [(88u32, "sess-decay", "/proj3")];
    let cwd_slug = "-proj3";
    let transcripts = [("sess-decay", cwd_slug, text_turn)];

    let mut provider = make_provider_with_sessions(base, &sessions, &transcripts);
    let procs = vec![make_proc(88, 0.0)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    assert!(
        out[&88].decay_score.is_some(),
        "decay_score should always be set for enriched sessions"
    );
}
