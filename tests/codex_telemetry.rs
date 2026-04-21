//! Integration tests for the Codex telemetry provider.
//!
//! These tests create a synthetic `~/.codex/` session tree under a `tempdir`
//! and drive `CodexTelemetryProvider::enrich` to verify end-to-end behaviour
//! without any real Codex process running.
//!
//! Test organisation:
//! 1. Fixture-based parser tests.
//! 2. Provider enrich tests using a synthetic session tree.
//! 3. Incremental tail / truncation tests via the provider.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};

use agentop::process::ProcessInfo;
use agentop::telemetry::codex::discovery::ymd_hms_to_secs;
use agentop::telemetry::codex::parser::{apply_items, RolloutAggregates};
use agentop::telemetry::codex::schema::RolloutItem;
use agentop::telemetry::{AgentStatus, CodexTelemetryProvider, TelemetryMap, TelemetryProvider};

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("codex")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("fixture {name} not found: {e}"))
}

fn parse_fixture_items(content: &str) -> Vec<RolloutItem> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse fixture line"))
        .collect()
}

// ---------------------------------------------------------------------------
// Fixture golden tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_minimal_session_parses_session_id() {
    let content = read_fixture("minimal_session.jsonl");
    let items = parse_fixture_items(&content);
    let mut agg = RolloutAggregates::default();
    apply_items(items, &mut agg);

    assert_eq!(
        agg.session_id.as_deref(),
        Some("00000001-0000-0000-0000-000000000001")
    );
    assert_eq!(agg.input_tokens, 0);
    assert!(agg.model.is_none());
}

#[test]
fn fixture_session_with_tokens_sets_all_fields() {
    let content = read_fixture("session_with_tokens.jsonl");
    let items = parse_fixture_items(&content);
    let mut agg = RolloutAggregates::default();
    apply_items(items, &mut agg);

    assert_eq!(agg.model.as_deref(), Some("gpt-5.4"));
    // Latest token_count wins (second one: 25000, not 11878).
    assert_eq!(agg.input_tokens, 25_000);
    assert_eq!(agg.cached_input_tokens, 15_000);
    assert_eq!(agg.output_tokens, 500);
    assert_eq!(agg.reasoning_output_tokens, 200);
    assert_eq!(agg.context_window, Some(258_400));
    assert!(agg.pending_tool.is_none());
}

#[test]
fn fixture_session_with_pending_tool_sets_pending_tool() {
    let content = read_fixture("session_with_pending_tool.jsonl");
    let items = parse_fixture_items(&content);
    let mut agg = RolloutAggregates::default();
    apply_items(items, &mut agg);

    assert_eq!(agg.pending_tool.as_deref(), Some("shell"));
    assert_eq!(agg.unresolved_call_ids.len(), 1);
    assert!(agg.unresolved_call_ids.contains("call-pending-001"));
}

#[test]
fn fixture_session_with_resolved_tool_clears_pending() {
    let content = read_fixture("session_with_resolved_tool.jsonl");
    let items = parse_fixture_items(&content);
    let mut agg = RolloutAggregates::default();
    apply_items(items, &mut agg);

    assert!(
        agg.pending_tool.is_none(),
        "tool was resolved; pending_tool should be None"
    );
    assert!(
        agg.unresolved_call_ids.is_empty(),
        "unresolved_call_ids should be empty after output"
    );
}

#[test]
fn fixture_truncated_parses_correctly() {
    let content = read_fixture("truncated.jsonl");
    let items = parse_fixture_items(&content);
    let mut agg = RolloutAggregates::default();
    apply_items(items, &mut agg);

    assert_eq!(agg.input_tokens, 42);
    assert_eq!(agg.output_tokens, 10);
}

// ---------------------------------------------------------------------------
// Provider integration tests with a synthetic session tree
// ---------------------------------------------------------------------------

/// The filename timestamp format for rollout files uses dashes instead of
/// colons: `rollout-YYYY-MM-DDTHH-MM-SS-{uuid}.jsonl`.
fn rollout_filename(process_start_secs: u64) -> String {
    let (y, m, d) = secs_to_ymd(process_start_secs);
    let h = ((process_start_secs % 86400) / 3600) as u32;
    let min = ((process_start_secs % 3600) / 60) as u32;
    let s = (process_start_secs % 60) as u32;
    format!("rollout-{y:04}-{m:02}-{d:02}T{h:02}-{min:02}-{s:02}-00000000-test-uuid-0000.jsonl")
}

/// Simple UTC seconds-to-YMD helper mirroring the discovery module.
fn secs_to_ymd(secs: u64) -> (u32, u32, u32) {
    let days = (secs / 86400) as u32;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Rewrite the `timestamp` and `payload.timestamp` on the first line of a
/// rollout fixture so it matches `process_start_secs` in UTC.
///
/// The PID-to-rollout correlation reads `session_meta.payload.timestamp`, not
/// the filename — so each test can use a distinct process start time without
/// maintaining a fixture per timestamp.
fn stamp_session_meta(content: &str, process_start_secs: u64) -> String {
    let (y, m, d) = secs_to_ymd(process_start_secs);
    let h = ((process_start_secs % 86400) / 3600) as u32;
    let min = ((process_start_secs % 3600) / 60) as u32;
    let s = (process_start_secs % 60) as u32;
    let ts = format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z");

    let mut lines = content.lines();
    let first = lines.next().unwrap_or("");
    let mut first_json: serde_json::Value =
        serde_json::from_str(first).expect("first line must be valid JSON");
    if first_json["type"].as_str() == Some("session_meta") {
        first_json["timestamp"] = serde_json::Value::String(ts.clone());
        if let Some(payload) = first_json["payload"].as_object_mut() {
            payload.insert("timestamp".to_string(), serde_json::Value::String(ts));
        }
    }
    let mut out = first_json.to_string();
    for line in lines {
        out.push('\n');
        out.push_str(line);
    }
    out.push('\n');
    out
}

/// Build a minimal synthetic `.codex/` tree and return a configured provider.
///
/// Creates:
/// - `{base}/sessions/{Y}/{M}/{D}/{rollout_filename}.jsonl` with the given
///   content, after rewriting its session_meta timestamp to match
///   `process_start_secs` so correlation finds it.
///
/// Returns the provider pointing at `base` as its CODEX_HOME.
fn make_provider_with_rollout(
    base: &Path,
    process_start_secs: u64,
    content: &str,
) -> (CodexTelemetryProvider, PathBuf) {
    let (y, m, d) = secs_to_ymd(process_start_secs);
    let day_dir = base
        .join("sessions")
        .join(format!("{y:04}"))
        .join(format!("{m:02}"))
        .join(format!("{d:02}"));
    std::fs::create_dir_all(&day_dir).unwrap();

    let filename = rollout_filename(process_start_secs);
    let rollout_path = day_dir.join(&filename);
    let stamped = stamp_session_meta(content, process_start_secs);
    std::fs::write(&rollout_path, stamped).unwrap();

    let provider = CodexTelemetryProvider::from_codex_home(base.to_path_buf());
    (provider, rollout_path)
}

fn make_codex_proc(pid: u32, cpu: f32, start_time: u64) -> ProcessInfo {
    ProcessInfo {
        pid,
        parent_pid: None,
        name: "codex".to_string(),
        cmd: vec!["codex".to_string()],
        exe_path: None,
        cwd: None,
        cpu_usage: cpu,
        memory_bytes: 0,
        status: "Run".to_string(),
        environ_count: 0,
        start_time,
        run_time: 10,
    }
}

#[test]
fn provider_enrich_reads_token_data() {
    let dir = tempfile::tempdir().unwrap();
    // Process start: 2026-04-21T10:43:38 UTC
    let start = ymd_hms_to_secs(2026, 4, 21, 10, 43, 38);
    let content = read_fixture("session_with_tokens.jsonl");

    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &content);
    let procs = vec![make_codex_proc(42, 0.0, start)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    assert_eq!(out.len(), 1);
    let tel = out.get(&42).unwrap();
    assert_eq!(tel.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(tel.input_tokens, Some(25_000));
    assert_eq!(tel.output_tokens, Some(500));
    assert_eq!(tel.cache_read_tokens, Some(15_000));
    assert!(tel.cost_usd.is_some(), "cost should be populated");
    assert!(tel.cost_is_estimated, "cost should be flagged as estimated");
    assert_eq!(tel.context_window, Some(258_400));
    assert!(tel.context_tokens.is_some());
    assert!(
        tel.decay_score.is_none(),
        "Codex sessions must not have a decay score"
    );
}

#[test]
fn provider_enrich_pending_tool() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 11, 0, 0);
    let content = read_fixture("session_with_pending_tool.jsonl");

    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &content);
    let procs = vec![make_codex_proc(77, 0.0, start)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    let tel = out.get(&77).unwrap();
    assert_eq!(tel.pending_tool.as_deref(), Some("shell"));
    assert_eq!(tel.status, Some(AgentStatus::NeedsInput));
}

#[test]
fn provider_enrich_resolved_tool_has_no_pending() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 11, 30, 0);
    let content = read_fixture("session_with_resolved_tool.jsonl");

    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &content);
    let procs = vec![make_codex_proc(88, 0.0, start)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    let tel = out.get(&88).unwrap();
    assert!(tel.pending_tool.is_none());
}

#[test]
fn provider_enrich_incremental_advance() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 12, 0, 0);
    // Start with only session_meta.
    let initial = read_fixture("minimal_session.jsonl");
    let (mut provider, rollout_path) = make_provider_with_rollout(dir.path(), start, &initial);

    let procs = vec![make_codex_proc(55, 0.0, start)];

    // First tick — only session_meta present.
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);
    let tel = out.get(&55).unwrap();
    assert_eq!(tel.input_tokens, None);

    // Append a token_count event.
    let token_line = r#"{"type":"event_msg","timestamp":"2026-04-21T12:00:05Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":500,"output_tokens":50,"reasoning_output_tokens":10,"total_tokens":1060},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":500,"output_tokens":50,"reasoning_output_tokens":10,"total_tokens":1060},"model_context_window":258400},"rate_limits":{}}}"#;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout_path)
        .unwrap();
    writeln!(f, "{token_line}").unwrap();
    drop(f);

    // Second tick — should pick up the new line.
    let mut out2 = TelemetryMap::new();
    provider.enrich(&procs, &mut out2);
    let tel2 = out2.get(&55).unwrap();
    assert_eq!(tel2.input_tokens, Some(1000));
    assert_eq!(tel2.output_tokens, Some(50));
}

#[test]
fn provider_enrich_truncation_resets() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 13, 0, 0);
    let content = read_fixture("truncated.jsonl");
    let (mut provider, rollout_path) = make_provider_with_rollout(dir.path(), start, &content);

    let procs = vec![make_codex_proc(33, 0.0, start)];

    // First tick.
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);
    assert_eq!(out[&33].input_tokens, Some(42));

    // Simulate truncation by overwriting with a smaller file.
    let shorter = r#"{"type":"session_meta","timestamp":"2026-04-21T13:00:00Z","payload":{"id":"trunc-new","timestamp":"2026-04-21T13:00:00Z","cwd":"/test","originator":"codex","cli_version":"0.116.0","source":"cli","model_provider":"openai","base_instructions":{"text":"x"}}}
{"type":"event_msg","timestamp":"2026-04-21T13:00:10Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":7,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"total_tokens":10},"last_token_usage":{"input_tokens":7,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"total_tokens":10},"model_context_window":258400},"rate_limits":{}}}
"#;
    std::fs::write(&rollout_path, shorter).unwrap();

    // Second tick — should detect truncation and restart from offset 0.
    let mut out2 = TelemetryMap::new();
    provider.enrich(&procs, &mut out2);
    // After reset, should reflect the new file's content, not the old.
    assert_eq!(out2[&33].input_tokens, Some(7));
}

#[test]
fn provider_prunes_dead_pids() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 14, 0, 0);
    let content = read_fixture("session_with_tokens.jsonl");
    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &content);

    // First tick with PID 99 alive.
    let procs = vec![make_codex_proc(99, 0.0, start)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);
    assert!(out.contains_key(&99));

    // Second tick with no processes — PID should be pruned.
    let mut out2 = TelemetryMap::new();
    provider.enrich(&[], &mut out2);
    assert!(!out2.contains_key(&99));
}

#[test]
fn provider_status_processing_when_high_cpu() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 15, 0, 0);
    let content = read_fixture("session_with_tokens.jsonl");
    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &content);

    let procs = vec![make_codex_proc(11, 50.0, start)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    assert_eq!(out[&11].status, Some(AgentStatus::Processing));
}

#[test]
fn provider_no_subagent_count_for_codex() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 16, 0, 0);
    let content = read_fixture("session_with_tokens.jsonl");
    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &content);

    let procs = vec![make_codex_proc(22, 0.0, start)];
    let mut out = TelemetryMap::new();
    provider.enrich(&procs, &mut out);

    assert_eq!(out[&22].subagent_count, 0);
}

/// Regression test: UTF-8 multibyte characters in rollout content must not
/// cause panics in the parser, mirroring the Claude telemetry regression test.
#[test]
fn provider_utf8_rollout_content_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let start = ymd_hms_to_secs(2026, 4, 21, 17, 0, 0);

    let chinese = "\u{4F60}\u{597D}\u{4E16}\u{754C}";
    let line = format!(
        r#"{{"type":"session_meta","timestamp":"2026-04-21T17:00:00Z","payload":{{"id":"utf8-test-id","timestamp":"2026-04-21T17:00:00Z","cwd":"/test","originator":"codex","cli_version":"0.116.0","source":"cli","model_provider":"openai","base_instructions":{{"text":"{chinese} em dash test"}}}}}}"#
    );

    let (mut provider, _) = make_provider_with_rollout(dir.path(), start, &line);
    let procs = vec![make_codex_proc(44, 0.0, start)];
    let mut out = TelemetryMap::new();
    // Must not panic.
    provider.enrich(&procs, &mut out);
}
