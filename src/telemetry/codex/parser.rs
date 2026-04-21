//! Incremental Codex rollout JSONL reader.
//!
//! [`RolloutTail`] owns a byte offset into the rollout file and only reads
//! new content on each call to [`RolloutTail::ingest`]. This keeps the
//! per-tick I/O cost proportional to the number of new events rather than
//! the total rollout size.
//!
//! # Truncation handling
//!
//! If the file is shorter than the stored offset (e.g. the file was rotated),
//! the offset is reset to 0 and the entire file is re-parsed.
//!
//! # Error tolerance
//!
//! Lines that fail to deserialize are skipped with a brief debug message
//! (only visible when `AGENTOP_DEBUG=1`). This prevents a single malformed
//! line from blocking telemetry for the entire session.
//!
//! # Token count strategy
//!
//! Codex rollout files contain cumulative `total_token_usage` in each
//! `token_count` event. Only the **latest** event's data is used — there is
//! no need to accumulate across events.

use std::collections::HashSet;
use std::io::{self, BufRead, Seek, SeekFrom};
use std::path::Path;

use super::schema::{EventMsgPayload, ResponseItemPayload, RolloutItem};

/// Accumulated telemetry derived from a Codex rollout file.
///
/// All fields are derived from the **latest** `token_count` event; older
/// events are superseded. The pending-tool set is maintained incrementally
/// across the file.
///
/// **Privacy**: this struct never stores raw rollout content (message text,
/// tool arguments, tool output) that could be serialized to `--json` output.
#[derive(Debug, Clone, Default)]
pub struct RolloutAggregates {
    /// Model from the most recent `turn_context` event.
    pub model: Option<String>,
    /// Cumulative input tokens from the latest `token_count` event.
    pub input_tokens: u64,
    /// Cumulative cached input tokens from the latest `token_count` event.
    pub cached_input_tokens: u64,
    /// Cumulative output tokens (excluding reasoning) from the latest event.
    pub output_tokens: u64,
    /// Cumulative reasoning output tokens from the latest event.
    pub reasoning_output_tokens: u64,
    /// Input tokens of the **most recent** model call (`last_token_usage.input_tokens`).
    /// This is the live "how full is the context" signal — `input_tokens`
    /// above is the session-cumulative counter, not the current prompt size.
    pub last_input_tokens: Option<u64>,
    /// Model context window size from the latest `token_count` event.
    pub context_window: Option<u64>,
    /// Unresolved tool call IDs: incremented on `function_call`/`custom_tool_call`,
    /// decremented on matching output events.
    pub unresolved_call_ids: HashSet<String>,
    /// Name of the most recently seen unresolved tool call, if any.
    pub pending_tool: Option<String>,
    /// Name of the most recent tool call (resolved or not).
    pub last_tool_call_name: Option<String>,
    /// Session ID from the `session_meta` line (first line only).
    pub session_id: Option<String>,
}

impl RolloutAggregates {
    /// Return the total tokens used (`input + cached + output + reasoning`).
    ///
    /// This mirrors the `total_tokens` field in the rollout for convenience.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.cached_input_tokens
            + self.output_tokens
            + self.reasoning_output_tokens
    }

    /// Return the context fraction — last-turn input size vs. window — clamped
    /// to `[0.0, 1.0]`. Returns `None` if `context_window` or
    /// `last_input_tokens` are absent, or if `context_window` is zero.
    ///
    /// `input_tokens` (cumulative) grows unbounded across turns and would
    /// quickly report values above 1.0 on any long session; the authoritative
    /// "current prompt size" comes from `last_token_usage.input_tokens`.
    pub fn context_fraction(&self) -> Option<f32> {
        let window = self.context_window? as f32;
        if window == 0.0 {
            return None;
        }
        let last_input = self.last_input_tokens? as f32;
        Some((last_input / window).clamp(0.0, 1.0))
    }
}

/// Incremental JSONL reader that tracks a byte offset into the rollout file.
///
/// Create one per session and call [`ingest`](RolloutTail::ingest) on every
/// telemetry tick. Only bytes after the stored offset are read.
#[derive(Debug, Default)]
pub struct RolloutTail {
    /// Byte offset of the first unread byte in the file. `0` means start.
    jsonl_offset: u64,
}

impl RolloutTail {
    /// Create a new tail starting at the beginning of the file.
    pub fn new() -> Self {
        Self { jsonl_offset: 0 }
    }

    /// Read new lines from `path` and update `agg` in place.
    ///
    /// - Seeks to the stored byte offset before reading.
    /// - If the file is shorter than the stored offset (truncation/rotation),
    ///   resets to offset 0 and re-parses the whole file.
    /// - Skips malformed lines without aborting.
    /// - Updates `jsonl_offset` to the new end-of-file position after parsing.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the `.jsonl` rollout file.
    /// * `agg`  - Accumulated aggregates to update in place.
    pub fn ingest(&mut self, path: &Path, agg: &mut RolloutAggregates) {
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };

        let file_len = match file.seek(SeekFrom::End(0)) {
            Ok(len) => len,
            Err(_) => return,
        };

        if file_len < self.jsonl_offset {
            debug_log("RolloutTail: truncation detected, resetting offset");
            self.jsonl_offset = 0;
            *agg = RolloutAggregates::default();
        }

        if file_len == self.jsonl_offset {
            return;
        }

        if file.seek(SeekFrom::Start(self.jsonl_offset)).is_err() {
            return;
        }

        let reader = io::BufReader::new(&mut file);
        let mut new_items: Vec<RolloutItem> = Vec::new();

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<RolloutItem>(trimmed) {
                Ok(item) => new_items.push(item),
                Err(e) => {
                    debug_log(&format!("RolloutTail: skipping malformed line: {e}"));
                }
            }
        }

        self.jsonl_offset = file_len;

        if !new_items.is_empty() {
            apply_items(new_items, agg);
        }
    }
}

/// Apply a batch of parsed rollout items to the aggregates.
///
/// This is extracted from `ingest` so it can be unit-tested independently.
pub fn apply_items(items: Vec<RolloutItem>, agg: &mut RolloutAggregates) {
    for item in items {
        match item {
            RolloutItem::SessionMeta(m) => {
                if agg.session_id.is_none() && !m.payload.id.is_empty() {
                    agg.session_id = Some(m.payload.id);
                }
            }

            RolloutItem::TurnContext(tc) => {
                // Prefer the model from collaboration_mode.settings if present.
                let model = tc
                    .payload
                    .collaboration_mode
                    .as_ref()
                    .and_then(|cm| cm.settings.as_ref())
                    .and_then(|s| s.model.clone())
                    .filter(|m| !m.is_empty())
                    .or_else(|| {
                        if !tc.payload.model.is_empty() {
                            Some(tc.payload.model.clone())
                        } else {
                            None
                        }
                    });

                if let Some(m) = model {
                    agg.model = Some(m);
                }
            }

            RolloutItem::EventMsg(em) => {
                if let EventMsgPayload::TokenCount(tc) = em.payload {
                    if let Some(info) = tc.info {
                        // Always replace with the latest cumulative totals.
                        agg.input_tokens = info.total_token_usage.input_tokens;
                        agg.cached_input_tokens = info.total_token_usage.cached_input_tokens;
                        agg.output_tokens = info.total_token_usage.output_tokens;
                        agg.reasoning_output_tokens =
                            info.total_token_usage.reasoning_output_tokens;
                        // Capture the per-turn input size so context_fraction
                        // reflects the live prompt, not the grand total.
                        if let Some(last) = info.last_token_usage.as_ref() {
                            agg.last_input_tokens = Some(last.input_tokens);
                        }
                        if info.model_context_window > 0 {
                            agg.context_window = Some(info.model_context_window);
                        }
                    }
                }
            }

            RolloutItem::ResponseItem(ri) => match ri.payload {
                ResponseItemPayload::FunctionCall(fc) | ResponseItemPayload::CustomToolCall(fc) => {
                    if !fc.call_id.is_empty() {
                        agg.unresolved_call_ids.insert(fc.call_id.clone());
                    }
                    if !fc.name.is_empty() {
                        agg.last_tool_call_name = Some(fc.name);
                    }
                }
                ResponseItemPayload::FunctionCallOutput(fco)
                | ResponseItemPayload::CustomToolCallOutput(fco) => {
                    agg.unresolved_call_ids.remove(&fco.call_id);
                }
                ResponseItemPayload::Other => {}
            },

            RolloutItem::Compacted(_) | RolloutItem::Unknown => {
                // Ignored for telemetry purposes.
            }
        }
    }

    // Recompute pending_tool: an unresolved call exists iff `unresolved_call_ids` is non-empty.
    // Use `last_tool_call_name` as the displayed name when there are pending calls.
    agg.pending_tool = if agg.unresolved_call_ids.is_empty() {
        None
    } else {
        // Use the most recently named tool for display.
        agg.last_tool_call_name.clone()
    };
}

/// Emit a debug message to stderr when `AGENTOP_DEBUG=1`.
fn debug_log(msg: &str) {
    if std::env::var("AGENTOP_DEBUG").as_deref() == Ok("1") {
        eprintln!("[agentop] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(path: &std::path::Path, lines: &[&str]) {
        let mut f = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    const SESSION_META: &str = r#"{"type":"session_meta","timestamp":"2026-04-21T10:00:00Z","payload":{"id":"019d2d54-0000-0000-0000-000000000001","timestamp":"2026-04-21T10:00:00Z","cwd":"/test","originator":"codex_cli_rs","cli_version":"0.116.0","source":"cli","model_provider":"openai","base_instructions":{"text":"test"}}}"#;

    const TURN_CONTEXT: &str = r#"{"type":"turn_context","timestamp":"2026-04-21T10:00:01Z","payload":{"turn_id":"t1","cwd":"/test","model":"gpt-5.4","approval_policy":"never","sandbox_policy":{"type":"danger-full-access"},"collaboration_mode":{"mode":"default","settings":{"model":"gpt-5.4","reasoning_effort":"high"}}}}"#;

    const TOKEN_COUNT_NULL: &str = r#"{"type":"event_msg","timestamp":"2026-04-21T10:00:02Z","payload":{"type":"token_count","info":null,"rate_limits":{}}}"#;

    const TOKEN_COUNT_FULL: &str = r#"{"type":"event_msg","timestamp":"2026-04-21T10:00:03Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":59182,"cached_input_tokens":23296,"output_tokens":740,"reasoning_output_tokens":152,"total_tokens":59922},"last_token_usage":{"input_tokens":59182,"cached_input_tokens":23296,"output_tokens":740,"reasoning_output_tokens":152,"total_tokens":59922},"model_context_window":258400},"rate_limits":{}}}"#;

    const FUNCTION_CALL: &str = r#"{"type":"response_item","timestamp":"2026-04-21T10:00:04Z","payload":{"type":"function_call","name":"shell","call_id":"call-001","arguments":"{}"}}"#;

    const FUNCTION_CALL_OUTPUT: &str = r#"{"type":"response_item","timestamp":"2026-04-21T10:00:05Z","payload":{"type":"function_call_output","call_id":"call-001","output":"done"}}"#;

    #[test]
    fn ingest_empty_file_is_handled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.input_tokens, 0);
        assert!(agg.model.is_none());
    }

    #[test]
    fn ingest_session_meta_sets_session_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[SESSION_META]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(
            agg.session_id.as_deref(),
            Some("019d2d54-0000-0000-0000-000000000001")
        );
    }

    #[test]
    fn ingest_turn_context_sets_model() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[TURN_CONTEXT]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.model.as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn ingest_null_token_count_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[TOKEN_COUNT_NULL]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.input_tokens, 0);
    }

    #[test]
    fn ingest_full_token_count_sets_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[TOKEN_COUNT_FULL]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.input_tokens, 59182);
        assert_eq!(agg.cached_input_tokens, 23296);
        assert_eq!(agg.output_tokens, 740);
        assert_eq!(agg.reasoning_output_tokens, 152);
        assert_eq!(agg.context_window, Some(258400));
    }

    #[test]
    fn ingest_function_call_sets_pending_tool() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[FUNCTION_CALL]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.pending_tool.as_deref(), Some("shell"));
        assert_eq!(agg.unresolved_call_ids.len(), 1);
    }

    #[test]
    fn ingest_function_call_output_clears_pending_tool() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[FUNCTION_CALL, FUNCTION_CALL_OUTPUT]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert!(agg.pending_tool.is_none());
        assert!(agg.unresolved_call_ids.is_empty());
    }

    #[test]
    fn offset_advances_between_calls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[SESSION_META]);
        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        let offset_after_first = tail.jsonl_offset;
        assert!(offset_after_first > 0);

        // Append a token_count line.
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "{TOKEN_COUNT_FULL}").unwrap();
        drop(f);

        tail.ingest(&path, &mut agg);
        assert!(tail.jsonl_offset > offset_after_first);
        assert_eq!(agg.input_tokens, 59182);
    }

    #[test]
    fn truncation_resets_offset_and_aggregates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(&path, &[TOKEN_COUNT_FULL]);

        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.input_tokens, 59182);

        // Simulate truncation by forcing offset beyond file length.
        tail.jsonl_offset = 999_999;
        tail.ingest(&path, &mut agg);
        // After reset, tokens should reflect only the file's content.
        assert_eq!(agg.input_tokens, 59182, "should recount after truncation");
        assert!(tail.jsonl_offset < 999_999, "offset should be reset");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        write_jsonl(
            &path,
            &[
                TOKEN_COUNT_FULL,
                r#"{"type": "event_msg", "MALFORMED JSON without closing"#,
                TOKEN_COUNT_FULL,
            ],
        );

        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        tail.ingest(&path, &mut agg);
        // The latest valid token count wins — still 59182.
        assert_eq!(agg.input_tokens, 59182);
    }

    #[test]
    fn context_fraction_computed_correctly() {
        // context_fraction now uses last-turn input size, not the cumulative
        // session total — long sessions would otherwise report > 100%.
        let agg = RolloutAggregates {
            input_tokens: 9_999_999,
            last_input_tokens: Some(100_000),
            context_window: Some(258_400),
            ..Default::default()
        };

        let frac = agg.context_fraction().unwrap();
        let expected = 100_000.0_f32 / 258_400.0_f32;
        assert!((frac - expected).abs() < 1e-5);
    }

    #[test]
    fn context_fraction_none_without_last_input() {
        // Without a last_token_usage value, we cannot say how full the window
        // was on the most recent turn.
        let agg = RolloutAggregates {
            input_tokens: 50_000,
            context_window: Some(258_400),
            ..Default::default()
        };
        assert!(agg.context_fraction().is_none());
    }

    #[test]
    fn context_fraction_none_when_window_missing() {
        let agg = RolloutAggregates::default();
        assert!(agg.context_fraction().is_none());
    }

    /// Regression test: UTF-8 multi-byte chars must not cause a byte-index panic
    /// in the JSONL parser.
    #[test]
    fn ingest_utf8_content_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");

        // A session_meta line with a non-ASCII base_instructions text.
        // Actual Unicode chars are embedded via Rust escape sequences in the string.
        // The resulting value is valid UTF-8 and valid JSON (the JSON serialized
        // form contains the literal characters, not escape sequences).
        let chinese = "\u{4F60}\u{597D}\u{4E16}\u{754C}";
        let line = format!(
            r#"{{"type":"session_meta","timestamp":"2026-04-21T10:00:00Z","payload":{{"id":"019d-utf8","timestamp":"2026-04-21T10:00:00Z","cwd":"/test","originator":"codex","cli_version":"0.116.0","source":"cli","model_provider":"openai","base_instructions":{{"text":"{chinese} em dash test"}}}}}}"#
        );
        write_jsonl(&path, &[&line]);

        let mut tail = RolloutTail::new();
        let mut agg = RolloutAggregates::default();
        // Must not panic.
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.session_id.as_deref(), Some("019d-utf8"));
    }
}
