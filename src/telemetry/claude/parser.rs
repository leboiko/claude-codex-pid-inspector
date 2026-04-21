//! Incremental JSONL transcript parser.
//!
//! [`TranscriptTail`] owns a byte offset into the transcript file and only
//! reads new content on each call to [`TranscriptTail::ingest`]. This keeps
//! the per-tick I/O cost proportional to the number of new events rather than
//! the total transcript size.
//!
//! # Truncation handling
//!
//! If the file is shorter than the stored offset (e.g. the transcript was
//! rotated), the offset is reset to 0 and the entire file is re-parsed.
//!
//! # Error tolerance
//!
//! Lines that fail to deserialize are skipped with a brief stderr message
//! (only visible when `AGENTOP_DEBUG=1`). This prevents a single malformed
//! line from blocking telemetry for the entire session.

use std::io::{self, BufRead, Seek, SeekFrom};
use std::path::Path;

use super::pricing::{compute_cost, lookup as lookup_pricing};
use super::schema::{
    AssistantEvent, ContentBlock, RecentMessage, TranscriptEvent, UserContentBlock,
};

/// Maximum number of recent assistant messages retained for the detail view.
const MAX_RECENT_MESSAGES: usize = 5;

/// Accumulated telemetry derived from a single session's transcript.
///
/// All fields are computed incrementally as new lines are parsed. Token
/// counters are cumulative totals; [`context_tokens`](TranscriptAggregates::context_tokens)
/// reflects the **most recent** assistant turn only (the best proxy for
/// current context size).
///
/// **Privacy**: this struct never stores raw message content that could be
/// serialized to `--json` output. The `recent_messages` field is only used
/// by the TUI detail view and is explicitly excluded from all JSON serialisation
/// paths. See `output/json.rs` for the serialisation boundary documentation.
#[derive(Debug, Clone, Default)]
pub struct TranscriptAggregates {
    /// Model identifier from the most recent assistant message.
    pub model: Option<String>,
    /// Cumulative input token count across all assistant turns.
    pub input_tokens: u64,
    /// Cumulative output token count across all assistant turns.
    pub output_tokens: u64,
    /// Cumulative cache-write token count across all assistant turns.
    pub cache_write_tokens: u64,
    /// Cumulative cache-read token count across all assistant turns.
    pub cache_read_tokens: u64,
    /// Context token estimate from the most recent assistant turn:
    /// `input + cache_creation + cache_read` on that turn.
    pub context_tokens: Option<u64>,
    /// Accumulated session cost in USD.
    pub cost_usd: f64,
    /// Tool name that is currently awaiting a result, if any.
    pub pending_tool: Option<String>,
    /// Stop reason from the most recent assistant turn.
    pub last_stop_reason: Option<String>,
    /// Timestamp string from the most recent assistant turn.
    pub last_event_timestamp: Option<String>,
    /// Recent assistant messages, newest last (up to `MAX_RECENT_MESSAGES`).
    ///
    /// Used only by the TUI detail view. **Must not be serialized to JSON.**
    pub recent_messages: Vec<RecentMessage>,
}

/// Incremental JSONL reader that tracks a byte offset into the transcript file.
///
/// Create one per session and call [`ingest`](TranscriptTail::ingest) on every
/// telemetry tick. Only bytes after the stored offset are read.
#[derive(Debug, Default)]
pub struct TranscriptTail {
    /// Byte offset of the first unread byte in the file. `0` means start.
    jsonl_offset: u64,
}

impl TranscriptTail {
    /// Create a new tail starting at the beginning of the file.
    pub fn new() -> Self {
        Self { jsonl_offset: 0 }
    }

    /// Read new lines from `path` and update `agg` in place.
    ///
    /// - Seeks to the stored byte offset before reading.
    /// - If the file is shorter than the stored offset (truncation / rotation),
    ///   resets to offset 0 and re-parses the whole file.
    /// - Skips malformed lines (invalid UTF-8 or JSON parse errors) without
    ///   aborting; emits a debug message when `AGENTOP_DEBUG=1`.
    /// - Updates `jsonl_offset` to the new end-of-file position after parsing.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the `.jsonl` transcript file.
    /// * `agg`  - Accumulated aggregates to update in place.
    pub fn ingest(&mut self, path: &Path, agg: &mut TranscriptAggregates) {
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };

        // Detect file size to check for truncation.
        let file_len = match file.seek(SeekFrom::End(0)) {
            Ok(len) => len,
            Err(_) => return,
        };

        if file_len < self.jsonl_offset {
            // File was truncated or rotated — reset and re-scan from the start.
            debug_log("TranscriptTail: truncation detected, resetting offset");
            self.jsonl_offset = 0;
            *agg = TranscriptAggregates::default();
        }

        if file_len == self.jsonl_offset {
            // Nothing new to read.
            return;
        }

        // Seek to the stored offset.
        if file.seek(SeekFrom::Start(self.jsonl_offset)).is_err() {
            return;
        }

        let reader = io::BufReader::new(&mut file);
        let mut lines_parsed: usize = 0;

        // Track unresolved tool-use IDs seen in this parse window.
        // We rebuild the pending-tool state from scratch on each ingest so that
        // resolved tool calls are correctly cleared even if they were in a
        // previously-parsed chunk.
        let mut new_events: Vec<TranscriptEvent> = Vec::new();

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<TranscriptEvent>(trimmed) {
                Ok(ev) => new_events.push(ev),
                Err(e) => {
                    debug_log(&format!("TranscriptTail: skipping malformed line: {e}"));
                }
            }
            lines_parsed += 1;
        }

        // Advance offset to the new file length.
        self.jsonl_offset = file_len;

        if lines_parsed == 0 && new_events.is_empty() {
            return;
        }

        apply_events(new_events, agg);
    }
}

/// Apply a batch of parsed events to the aggregates.
///
/// This is separate from `ingest` so it can be unit-tested independently.
pub fn apply_events(events: Vec<TranscriptEvent>, agg: &mut TranscriptAggregates) {
    // Collect all tool_use_ids that have been resolved (appear in user tool_result blocks).
    let mut resolved_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for ev in &events {
        if let TranscriptEvent::User(user) = ev {
            for block in &user.message.content {
                if let UserContentBlock::ToolResult { tool_use_id } = block {
                    if !tool_use_id.is_empty() {
                        resolved_ids.insert(tool_use_id.clone());
                    }
                }
            }
        }
    }

    let mut last_assistant: Option<&AssistantEvent> = None;

    for ev in &events {
        if let TranscriptEvent::Assistant(a) = ev {
            let usage = &a.message.usage;
            let model = a.message.model.as_str();
            let pricing = lookup_pricing(model);

            // Accumulate token totals.
            agg.input_tokens += usage.input_tokens;
            agg.output_tokens += usage.output_tokens;
            agg.cache_write_tokens += usage.cache_creation_input_tokens;
            agg.cache_read_tokens += usage.cache_read_input_tokens;

            // Accumulate cost.
            agg.cost_usd += compute_cost(
                &pricing,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens,
                usage.cache_read_input_tokens,
            );

            // Update model (from most recent message).
            if !model.is_empty() {
                agg.model = Some(model.to_string());
            }

            // Update context tokens from this turn: input + cache_creation + cache_read.
            let ctx = usage.input_tokens
                + usage.cache_creation_input_tokens
                + usage.cache_read_input_tokens;
            agg.context_tokens = Some(ctx);

            // Update stop reason and timestamp.
            agg.last_stop_reason = a.message.stop_reason.clone();
            if !a.timestamp.is_empty() {
                agg.last_event_timestamp = Some(a.timestamp.clone());
            }

            last_assistant = Some(a);

            // Add to recent messages (capped at MAX_RECENT_MESSAGES).
            let preview = build_preview(&a.message.content);
            let rm = RecentMessage {
                timestamp: a.timestamp.clone(),
                model: model.to_string(),
                stop_reason: a.message.stop_reason.clone(),
                preview,
            };
            if agg.recent_messages.len() >= MAX_RECENT_MESSAGES {
                agg.recent_messages.remove(0);
            }
            agg.recent_messages.push(rm);
        }
    }

    // Recompute pending_tool from the last assistant event in this batch.
    // A pending tool exists when:
    //   1. The last assistant event has stop_reason == "tool_use".
    //   2. It contains a tool_use block whose id was NOT resolved in a
    //      subsequent user event within this batch.
    if let Some(last) = last_assistant {
        let pending = if last.message.stop_reason.as_deref() == Some("tool_use") {
            last.message
                .content
                .iter()
                .filter_map(ContentBlock::tool_use_id)
                .filter(|id| !resolved_ids.contains(*id))
                .filter_map(|id| {
                    // Find the tool name for this unresolved id.
                    last.message
                        .content
                        .iter()
                        .find(|b| b.tool_use_id() == Some(id))
                        .and_then(|b| b.tool_name())
                        .map(str::to_string)
                })
                .next()
        } else {
            None
        };
        agg.pending_tool = pending;
    }
}

/// Build a short preview string for a content block list.
///
/// Returns the first ~80 chars of text, or `[thinking]` / `[tool_use: Name]`
/// for non-text blocks. If no displayable content exists, returns `"(empty)"`.
fn build_preview(content: &[ContentBlock]) -> String {
    for block in content {
        match block {
            ContentBlock::Text { text } if !text.is_empty() => {
                // Char-based truncation — byte-indexing into UTF-8 text would
                // panic if 80 fell inside a multi-byte grapheme (e.g. an em
                // dash occupies 3 bytes).
                let truncated = if text.chars().count() > 80 {
                    let head: String = text.chars().take(80).collect();
                    format!("{head}...")
                } else {
                    text.clone()
                };
                // Replace newlines for single-line display.
                return truncated.replace('\n', " ");
            }
            ContentBlock::Thinking { .. } => return "[thinking]".to_string(),
            ContentBlock::ToolUse { name, .. } => {
                return format!("[tool_use: {}]", name);
            }
            _ => {}
        }
    }
    "(empty)".to_string()
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
            writeln!(f, "{}", line).unwrap();
        }
    }

    const ASSISTANT_TEXT: &str = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-04-21T10:00:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"Hello there!"}]}}"#;

    const ASSISTANT_TOOL_USE: &str = r#"{"type":"assistant","uuid":"a2","timestamp":"2026-04-21T10:01:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"tool_use","usage":{"input_tokens":200,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"tool_use","id":"tu_001","name":"Bash","input":{}}]}}"#;

    const USER_TOOL_RESULT: &str = r#"{"type":"user","uuid":"u1","timestamp":"2026-04-21T10:01:05Z","message":{"content":[{"type":"tool_result","tool_use_id":"tu_001","content":"output"}]}}"#;

    #[test]
    fn ingest_text_turn_accumulates_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        write_jsonl(&path, &[ASSISTANT_TEXT]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);

        assert_eq!(agg.input_tokens, 100);
        assert_eq!(agg.output_tokens, 50);
        assert_eq!(agg.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(agg.last_stop_reason.as_deref(), Some("end_turn"));
        assert!(agg.pending_tool.is_none());
    }

    #[test]
    fn pending_tool_set_when_unresolved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        write_jsonl(&path, &[ASSISTANT_TOOL_USE]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);

        assert_eq!(agg.pending_tool.as_deref(), Some("Bash"));
    }

    #[test]
    fn pending_tool_cleared_when_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        write_jsonl(&path, &[ASSISTANT_TOOL_USE, USER_TOOL_RESULT]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);

        assert!(
            agg.pending_tool.is_none(),
            "tool was resolved, should be None"
        );
    }

    #[test]
    fn offset_advances_between_calls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        write_jsonl(&path, &[ASSISTANT_TEXT]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);
        let offset_after_first = tail.jsonl_offset;
        assert!(offset_after_first > 0);

        // Append a second event.
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "{}", ASSISTANT_TEXT).unwrap();
        drop(f);

        tail.ingest(&path, &mut agg);
        assert!(
            tail.jsonl_offset > offset_after_first,
            "offset should have advanced"
        );
        // Tokens should have doubled.
        assert_eq!(agg.input_tokens, 200);
        assert_eq!(agg.output_tokens, 100);
    }

    #[test]
    fn truncation_resets_offset_and_aggregates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        write_jsonl(&path, &[ASSISTANT_TEXT]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);
        assert!(tail.jsonl_offset > 0);
        assert_eq!(agg.input_tokens, 100);

        // Truncate the file (simulate rotation by overwriting with a single short line).
        write_jsonl(&path, &[ASSISTANT_TEXT]);
        // Manually force offset beyond file length to simulate truncation.
        tail.jsonl_offset = 999_999;

        tail.ingest(&path, &mut agg);
        // After reset, tokens should reflect only the new file content.
        assert_eq!(
            agg.input_tokens, 100,
            "should recount after truncation reset"
        );
        assert!(tail.jsonl_offset < 999_999, "offset should be reset");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        write_jsonl(
            &path,
            &[
                ASSISTANT_TEXT,
                r#"{"type": "assistant", "MALFORMED JSON without closing"#,
                ASSISTANT_TEXT,
            ],
        );

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);

        // Should have processed 2 valid lines, skipped 1 malformed.
        assert_eq!(agg.input_tokens, 200);
    }

    #[test]
    fn empty_file_is_handled_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        write_jsonl(&path, &[]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.input_tokens, 0);
        assert!(agg.model.is_none());
    }

    #[test]
    fn context_tokens_is_latest_turn_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        // Two turns with different input token counts.
        let turn2 = r#"{"type":"assistant","uuid":"a3","timestamp":"2026-04-21T10:02:00Z","message":{"model":"claude-sonnet-4-5","stop_reason":"end_turn","usage":{"input_tokens":500,"output_tokens":20,"cache_creation_input_tokens":100,"cache_read_input_tokens":50},"content":[]}}"#;
        write_jsonl(&path, &[ASSISTANT_TEXT, turn2]);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);

        // context_tokens = input(500) + cache_creation(100) + cache_read(50) = 650 (from last turn)
        assert_eq!(agg.context_tokens, Some(650));
        // Total input = 100 + 500 = 600
        assert_eq!(agg.input_tokens, 600);
    }

    #[test]
    fn recent_messages_capped_at_five() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        let events: Vec<&str> = vec![ASSISTANT_TEXT; 7];
        write_jsonl(&path, &events);

        let mut tail = TranscriptTail::new();
        let mut agg = TranscriptAggregates::default();
        tail.ingest(&path, &mut agg);
        assert_eq!(agg.recent_messages.len(), 5, "should keep only last 5");
    }

    /// Regression test: the preview builder must not panic when the 80-char
    /// boundary falls inside a multi-byte UTF-8 character (e.g. em dash, 3
    /// bytes). A raw byte slice like `&text[..80]` would have panicked here.
    #[test]
    fn preview_handles_multibyte_chars_at_truncation_boundary() {
        // 79 ASCII chars + em dash (3 bytes) + more text — byte index 80
        // lands inside the em dash.
        let prefix: String = "a".repeat(79);
        let text = format!("{prefix}— trailing content that forces truncation");

        let blocks = vec![ContentBlock::Text { text }];
        let preview = build_preview(&blocks);

        // Doesn't panic. Contains the prefix and ellipsis.
        assert!(preview.starts_with(&"a".repeat(79)));
        assert!(preview.contains("..."));
    }
}
