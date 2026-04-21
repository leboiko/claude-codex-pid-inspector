//! Serde types for Codex rollout JSONL events.
//!
//! # Event format
//!
//! Every line in a rollout file has the shape:
//! ```json
//! {"type": "<kind>", "timestamp": "<rfc3339>", "payload": {...}}
//! ```
//!
//! The `type` field selects the [`RolloutItem`] variant. Unknown types are
//! folded into [`RolloutItem::Unknown`] via `#[serde(other)]` so that future
//! Codex protocol additions do not break existing sessions.
//!
//! # Privacy
//!
//! Types in this module that carry user content (message text, tool arguments,
//! tool outputs) are intentionally structured so that only the fields needed
//! for telemetry are retained. Tool `arguments` and `output` fields use
//! `#[serde(skip)]` — they are never deserialized into memory and can never
//! accidentally appear in `--json` output.

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Top-level rollout line
// ---------------------------------------------------------------------------

/// A single line from a Codex rollout JSONL file.
///
/// The `type` field selects the variant. Unknown types are silently folded
/// into [`RolloutItem::Unknown`] without error.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RolloutItem {
    /// Initial session metadata (first line of every rollout file).
    SessionMeta(SessionMetaItem),
    /// Per-turn context record.
    TurnContext(TurnContextItem),
    /// A tagged event message (token counts, task events, etc.).
    EventMsg(EventMsgItem),
    /// A model response item (messages, function calls, reasoning, etc.).
    ResponseItem(ResponseItemPayloadWrapper),
    /// A context compaction boundary marker.
    Compacted(CompactedItem),
    /// Any unrecognised type — ignored.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// session_meta
// ---------------------------------------------------------------------------

/// The `session_meta` rollout line (first line of every session file).
#[derive(Debug, Deserialize)]
pub struct SessionMetaItem {
    /// RFC 3339 timestamp of this line.
    #[serde(default)]
    pub timestamp: String,
    /// Session metadata payload.
    pub payload: SessionMetaPayload,
}

/// Payload of the `session_meta` line.
#[derive(Debug, Deserialize)]
pub struct SessionMetaPayload {
    /// Unique session UUID (matches the UUID in the filename).
    #[serde(default)]
    pub id: String,
    /// Session start timestamp (RFC 3339).
    #[serde(default)]
    pub timestamp: String,
    /// Working directory of the Codex process at session start.
    #[serde(default)]
    pub cwd: String,
    /// Codex CLI version string (e.g. `"0.116.0"`).
    #[serde(default)]
    pub cli_version: String,
    /// Model provider (e.g. `"openai"`).
    #[serde(default)]
    pub model_provider: String,
}

// ---------------------------------------------------------------------------
// turn_context
// ---------------------------------------------------------------------------

/// The `turn_context` rollout line, written once per model turn.
#[derive(Debug, Deserialize)]
pub struct TurnContextItem {
    /// RFC 3339 timestamp of this line.
    #[serde(default)]
    pub timestamp: String,
    /// Turn context payload.
    pub payload: TurnContextPayload,
}

/// Payload of the `turn_context` line.
#[derive(Debug, Deserialize)]
pub struct TurnContextPayload {
    /// Model identifier for this turn (e.g. `"gpt-5.4"`).
    #[serde(default)]
    pub model: String,
    /// Collaboration mode, which may carry a more-specific model setting.
    #[serde(default)]
    pub collaboration_mode: Option<CollaborationMode>,
}

/// The collaboration mode block in a `turn_context` payload.
#[derive(Debug, Deserialize, Default)]
pub struct CollaborationMode {
    /// Mode name (`"default"`, `"plan"`, etc.).
    #[serde(default)]
    pub mode: String,
    /// Per-mode settings.
    #[serde(default)]
    pub settings: Option<CollaborationSettings>,
}

/// Per-mode settings inside a collaboration mode block.
#[derive(Debug, Deserialize, Default)]
pub struct CollaborationSettings {
    /// Model override within this collaboration mode (more specific than `turn_context.model`).
    #[serde(default)]
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// event_msg
// ---------------------------------------------------------------------------

/// The `event_msg` rollout line.
#[derive(Debug, Deserialize)]
pub struct EventMsgItem {
    /// RFC 3339 timestamp of this line.
    #[serde(default)]
    pub timestamp: String,
    /// The tagged event payload.
    pub payload: EventMsgPayload,
}

/// Payload of an `event_msg` line.
///
/// The inner `type` field selects the variant. Non-telemetry event types
/// are folded into [`EventMsgPayload::Other`].
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventMsgPayload {
    /// Token count update — the primary telemetry signal.
    TokenCount(TokenCountPayload),
    /// Any other event message type — ignored.
    #[serde(other)]
    Other,
}

/// Payload of a `token_count` event.
#[derive(Debug, Deserialize)]
pub struct TokenCountPayload {
    /// Cumulative and most-recent token usage. `None` on the first event
    /// (before the first model response).
    pub info: Option<TokenCountInfo>,
}

/// Token usage information inside a `token_count` event.
#[derive(Debug, Deserialize)]
pub struct TokenCountInfo {
    /// Cumulative token usage for the entire session.
    pub total_token_usage: TokenUsage,
    /// Per-turn usage for the most recent model call. `input_tokens` here is
    /// the authoritative measure of how much of the context window the last
    /// prompt consumed — the cumulative counter in `total_token_usage` grows
    /// unbounded across turns and should not be used for live "context full"
    /// signalling.
    #[serde(default)]
    pub last_token_usage: Option<TokenUsage>,
    /// Context window size for the active model.
    #[serde(default)]
    pub model_context_window: u64,
}

/// Token usage breakdown.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct TokenUsage {
    /// Non-cached input tokens.
    #[serde(default)]
    pub input_tokens: u64,
    /// Cached input tokens.
    #[serde(default)]
    pub cached_input_tokens: u64,
    /// Output tokens (excluding reasoning).
    #[serde(default)]
    pub output_tokens: u64,
    /// Reasoning output tokens (billed at the output rate).
    #[serde(default)]
    pub reasoning_output_tokens: u64,
    /// Total tokens (`input + cached + output + reasoning`).
    #[serde(default)]
    pub total_tokens: u64,
}

// ---------------------------------------------------------------------------
// response_item
// ---------------------------------------------------------------------------

/// Wrapper for the `response_item` rollout line.
#[derive(Debug, Deserialize)]
pub struct ResponseItemPayloadWrapper {
    /// RFC 3339 timestamp of this line.
    #[serde(default)]
    pub timestamp: String,
    /// The tagged response payload.
    pub payload: ResponseItemPayload,
}

/// Payload of a `response_item` line.
///
/// The inner `type` field selects the variant. Types we do not need for
/// telemetry (`reasoning`, and anything unrecognised) are folded into
/// [`ResponseItemPayload::Other`].
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseItemPayload {
    /// A tool invocation request from the model.
    FunctionCall(FunctionCallPayload),
    /// The result of a tool invocation.
    FunctionCallOutput(FunctionCallOutputPayload),
    /// A custom (shell) tool invocation request.
    CustomToolCall(FunctionCallPayload),
    /// The result of a custom tool invocation.
    CustomToolCallOutput(FunctionCallOutputPayload),
    /// A model / user / developer message. Only the role and the text of
    /// output_text blocks are captured; input_text content (prompts, file
    /// dumps) is deliberately read but only surfaces in the in-process
    /// RecentMessage ring buffer for the TUI detail view — never in
    /// `--json` output.
    Message(MessagePayload),
    /// Any other response item type — ignored.
    #[serde(other)]
    Other,
}

/// Payload of a `response_item.message` event.
#[derive(Debug, Deserialize)]
pub struct MessagePayload {
    /// `"assistant"`, `"user"`, or `"developer"`.
    #[serde(default)]
    pub role: String,
    /// Content blocks. Most assistant messages have exactly one
    /// `output_text` block; user/developer messages carry `input_text`.
    #[serde(default)]
    pub content: Vec<MessageContentBlock>,
}

/// One content block inside a `response_item.message`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContentBlock {
    /// Text the model generated.
    OutputText {
        #[serde(default)]
        text: String,
    },
    /// Text sent into the model (prompts, file contents).
    InputText {
        #[serde(default)]
        text: String,
    },
    /// Any other block type (images, reasoning summaries, ...).
    #[serde(other)]
    Other,
}

/// Payload of a `function_call` or `custom_tool_call` response item.
///
/// **Privacy**: `arguments` is intentionally absent — tool input data must
/// never be deserialized or included in `--json` output.
#[derive(Debug, Deserialize)]
pub struct FunctionCallPayload {
    /// The tool name (e.g. `"shell"`, `"apply_patch"`).
    #[serde(default)]
    pub name: String,
    /// Stable call identifier used to match with the corresponding output.
    #[serde(default)]
    pub call_id: String,
    // `arguments` is deliberately skipped — privacy boundary.
}

/// Payload of a `function_call_output` or `custom_tool_call_output` response item.
///
/// **Privacy**: `output` is intentionally absent — tool output data must
/// never be deserialized or included in `--json` output.
#[derive(Debug, Deserialize)]
pub struct FunctionCallOutputPayload {
    /// The `call_id` from the matching `function_call` / `custom_tool_call`.
    #[serde(default)]
    pub call_id: String,
    // `output` is deliberately skipped — privacy boundary.
}

// ---------------------------------------------------------------------------
// compacted
// ---------------------------------------------------------------------------

/// The `compacted` rollout line — a context compaction boundary marker.
///
/// No fields are needed for telemetry; the line is parsed to avoid treating
/// it as unknown.
#[derive(Debug, Deserialize)]
pub struct CompactedItem {
    /// RFC 3339 timestamp of this line.
    #[serde(default)]
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> RolloutItem {
        serde_json::from_str(json).expect("parse failed")
    }

    #[test]
    fn parses_session_meta() {
        let json = r#"{
            "type": "session_meta",
            "timestamp": "2026-04-21T10:00:00Z",
            "payload": {
                "id": "019d2d54-207e-7b80-b98e-2968e4d1204a",
                "timestamp": "2026-04-21T10:00:00Z",
                "cwd": "/Users/me/project",
                "originator": "codex_cli_rs",
                "cli_version": "0.116.0",
                "source": "cli",
                "model_provider": "openai",
                "base_instructions": {"text": "you are codex"}
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::SessionMeta(m) => {
                assert_eq!(m.payload.id, "019d2d54-207e-7b80-b98e-2968e4d1204a");
                assert_eq!(m.payload.cwd, "/Users/me/project");
                assert_eq!(m.payload.cli_version, "0.116.0");
                assert_eq!(m.payload.model_provider, "openai");
            }
            other => panic!("expected SessionMeta, got {other:?}"),
        }
    }

    #[test]
    fn parses_turn_context() {
        let json = r#"{
            "type": "turn_context",
            "timestamp": "2026-04-21T10:00:01Z",
            "payload": {
                "turn_id": "turn-001",
                "cwd": "/Users/me/project",
                "model": "gpt-5.4",
                "approval_policy": "never",
                "sandbox_policy": {"type": "danger-full-access"},
                "collaboration_mode": {
                    "mode": "default",
                    "settings": {"model": "gpt-5.4", "reasoning_effort": "high"}
                }
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::TurnContext(tc) => {
                assert_eq!(tc.payload.model, "gpt-5.4");
                let mode = tc.payload.collaboration_mode.unwrap();
                let settings = mode.settings.unwrap();
                assert_eq!(settings.model.as_deref(), Some("gpt-5.4"));
            }
            other => panic!("expected TurnContext, got {other:?}"),
        }
    }

    #[test]
    fn parses_token_count_event_with_info() {
        let json = r#"{
            "type": "event_msg",
            "timestamp": "2026-04-21T10:00:02Z",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": 59182,
                        "cached_input_tokens": 23296,
                        "output_tokens": 740,
                        "reasoning_output_tokens": 152,
                        "total_tokens": 59922
                    },
                    "last_token_usage": {
                        "input_tokens": 59182,
                        "cached_input_tokens": 23296,
                        "output_tokens": 740,
                        "reasoning_output_tokens": 152,
                        "total_tokens": 59922
                    },
                    "model_context_window": 258400
                },
                "rate_limits": {"limit_id": "codex"}
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::EventMsg(e) => match e.payload {
                EventMsgPayload::TokenCount(tc) => {
                    let info = tc.info.expect("info should be present");
                    assert_eq!(info.total_token_usage.input_tokens, 59182);
                    assert_eq!(info.total_token_usage.cached_input_tokens, 23296);
                    assert_eq!(info.total_token_usage.output_tokens, 740);
                    assert_eq!(info.total_token_usage.reasoning_output_tokens, 152);
                    assert_eq!(info.model_context_window, 258400);
                }
                other => panic!("expected TokenCount payload, got {other:?}"),
            },
            other => panic!("expected EventMsg, got {other:?}"),
        }
    }

    #[test]
    fn parses_token_count_event_with_null_info() {
        let json = r#"{
            "type": "event_msg",
            "timestamp": "2026-04-21T10:00:02Z",
            "payload": {
                "type": "token_count",
                "info": null,
                "rate_limits": {}
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::EventMsg(e) => match e.payload {
                EventMsgPayload::TokenCount(tc) => {
                    assert!(tc.info.is_none());
                }
                other => panic!("expected TokenCount payload, got {other:?}"),
            },
            other => panic!("expected EventMsg, got {other:?}"),
        }
    }

    #[test]
    fn parses_function_call_response_item() {
        let json = r#"{
            "type": "response_item",
            "timestamp": "2026-04-21T10:00:03Z",
            "payload": {
                "type": "function_call",
                "name": "shell",
                "call_id": "call-001",
                "arguments": "{\"command\": \"ls -la\"}"
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::ResponseItem(ri) => match ri.payload {
                ResponseItemPayload::FunctionCall(fc) => {
                    assert_eq!(fc.name, "shell");
                    assert_eq!(fc.call_id, "call-001");
                }
                other => panic!("expected FunctionCall, got {other:?}"),
            },
            other => panic!("expected ResponseItem, got {other:?}"),
        }
    }

    #[test]
    fn parses_function_call_output_response_item() {
        let json = r#"{
            "type": "response_item",
            "timestamp": "2026-04-21T10:00:04Z",
            "payload": {
                "type": "function_call_output",
                "call_id": "call-001",
                "output": "file1.txt\nfile2.txt"
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::ResponseItem(ri) => match ri.payload {
                ResponseItemPayload::FunctionCallOutput(fco) => {
                    assert_eq!(fco.call_id, "call-001");
                }
                other => panic!("expected FunctionCallOutput, got {other:?}"),
            },
            other => panic!("expected ResponseItem, got {other:?}"),
        }
    }

    #[test]
    fn parses_compacted() {
        let json = r#"{"type": "compacted", "timestamp": "2026-04-21T10:00:10Z", "payload": {}}"#;
        let item = parse(json);
        assert!(matches!(item, RolloutItem::Compacted(_)));
    }

    #[test]
    fn unknown_type_becomes_unknown() {
        let json =
            r#"{"type": "some_future_type", "timestamp": "2026-04-21T10:00:00Z", "payload": {}}"#;
        let item = parse(json);
        assert!(matches!(item, RolloutItem::Unknown));
    }

    #[test]
    fn non_token_count_event_msg_becomes_other() {
        let json = r#"{
            "type": "event_msg",
            "timestamp": "2026-04-21T10:00:00Z",
            "payload": {"type": "task_started", "turn_id": "t1", "model_context_window": 258400}
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::EventMsg(e) => {
                assert!(matches!(e.payload, EventMsgPayload::Other));
            }
            other => panic!("expected EventMsg, got {other:?}"),
        }
    }

    #[test]
    fn custom_tool_call_parsed() {
        let json = r#"{
            "type": "response_item",
            "timestamp": "2026-04-21T10:00:05Z",
            "payload": {
                "type": "custom_tool_call",
                "name": "codex_shell",
                "call_id": "csc-001",
                "arguments": "{}"
            }
        }"#;
        let item = parse(json);
        match item {
            RolloutItem::ResponseItem(ri) => match ri.payload {
                ResponseItemPayload::CustomToolCall(fc) => {
                    assert_eq!(fc.name, "codex_shell");
                    assert_eq!(fc.call_id, "csc-001");
                }
                other => panic!("expected CustomToolCall, got {other:?}"),
            },
            other => panic!("expected ResponseItem, got {other:?}"),
        }
    }
}
