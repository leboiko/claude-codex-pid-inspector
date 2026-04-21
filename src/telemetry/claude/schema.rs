//! Serde types for Claude session metadata and transcript events.
//!
//! # Design choices
//!
//! - Unknown `type` values in transcript JSONL lines are deserialized into
//!   `TranscriptEvent::Other` using `#[serde(other)]` on a unit variant.
//!   This is the simplest approach and avoids storing arbitrary JSON blobs
//!   in memory for events we will never inspect. Unknown fields within known
//!   event types are silently dropped via `#[serde(deny_unknown_fields)]`
//!   being absent — i.e. extra fields are accepted and ignored.
//!
//! - All structs use `#[serde(default)]` at the field level (or struct level
//!   where appropriate) so that future Claude protocol additions do not break
//!   existing sessions.
//!
//! - `startedAt` in [`SessionMeta`] is milliseconds since Unix epoch (not
//!   seconds). We store it as `u64` and convert when needed.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Session metadata  (~/.claude/sessions/{pid}.json)
// ---------------------------------------------------------------------------

/// Metadata file written by Claude Code for each running session.
///
/// Deserialized from `~/.claude/sessions/{pid}.json`. The `pid` field maps
/// directly to an OS process ID, making correlation trivial.
///
/// Extra fields present in future Claude versions are silently ignored
/// (no `deny_unknown_fields`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    /// OS process ID that owns this session.
    pub pid: u32,
    /// Unique session UUID string.
    #[serde(default)]
    pub session_id: String,
    /// Working directory of the Claude process.
    #[serde(default)]
    pub cwd: String,
    /// Session start time in milliseconds since the Unix epoch.
    #[serde(default)]
    pub started_at: u64,
    /// Claude CLI version string (e.g. `"2.1.116"`).
    #[serde(default)]
    pub version: String,
    /// Peer protocol version integer.
    #[serde(default)]
    pub peer_protocol: u32,
    /// Session kind (`"interactive"`, etc.).
    #[serde(default)]
    pub kind: String,
    /// Entry point (`"cli"`, etc.).
    #[serde(default)]
    pub entrypoint: String,
}

// ---------------------------------------------------------------------------
// Transcript events  (~/.claude/projects/{cwd-slug}/{sessionId}.jsonl)
// ---------------------------------------------------------------------------

/// A single line from a Claude transcript JSONL file.
///
/// Only `assistant` and `user` events carry telemetry-relevant data. All
/// other event types are folded into [`TranscriptEvent::Other`] and ignored.
///
/// The `#[serde(tag = "type")]` attribute selects the variant based on the
/// `"type"` field present in every line. The `#[serde(other)]` on the `Other`
/// variant captures any unrecognised type without error.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptEvent {
    /// An assistant (model) response turn.
    #[serde(rename = "assistant")]
    Assistant(AssistantEvent),
    /// A user input turn (includes tool results).
    #[serde(rename = "user")]
    User(UserEvent),
    /// Any other event type — ignored.
    #[serde(other)]
    Other,
}

/// An assistant event from the transcript.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AssistantEvent {
    /// UUID of this event.
    #[serde(default)]
    pub uuid: String,
    /// RFC 3339 timestamp string.
    #[serde(default)]
    pub timestamp: String,
    /// The assistant message payload.
    #[serde(default)]
    pub message: AssistantMessage,
}

/// The `message` field inside an assistant event.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AssistantMessage {
    /// Model identifier (e.g. `"claude-opus-4-7"`).
    #[serde(default)]
    pub model: String,
    /// Stop reason (`"tool_use"`, `"end_turn"`, `null`, …).
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Token usage for this turn.
    #[serde(default)]
    pub usage: TokenUsage,
    /// Content blocks for this turn.
    #[serde(default)]
    pub content: Vec<ContentBlock>,
}

/// Token usage breakdown for one assistant turn.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TokenUsage {
    /// Input tokens (excluding cache).
    #[serde(default)]
    pub input_tokens: u64,
    /// Output tokens.
    #[serde(default)]
    pub output_tokens: u64,
    /// Tokens written to the cache this turn.
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    /// Tokens read from cache this turn.
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

/// A single content block inside a message.
///
/// Only `tool_use` blocks carry data we need; `text` and `thinking` blocks
/// are deserialized but their text body is not stored to avoid holding large
/// strings in memory.
///
/// The `#[serde(other)]` variant absorbs unknown block types.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// A plain text response block.
    #[serde(rename = "text")]
    Text {
        /// The text content. Stored for the detail-view "recent messages" panel.
        #[serde(default)]
        text: String,
    },
    /// An extended thinking block.
    #[serde(rename = "thinking")]
    Thinking {
        /// Thinking text. Not used for telemetry; present for completeness.
        #[serde(default)]
        thinking: String,
    },
    /// A tool invocation block.
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Stable unique identifier for this tool call.
        #[serde(default)]
        id: String,
        /// Tool name (e.g. `"Bash"`, `"Read"`, `"Write"`).
        #[serde(default)]
        name: String,
    },
    /// Any other block type — ignored.
    #[serde(other)]
    Other,
}

impl ContentBlock {
    /// Return the tool-use `id` if this is a [`ContentBlock::ToolUse`], otherwise `None`.
    pub fn tool_use_id(&self) -> Option<&str> {
        match self {
            ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }

    /// Return the tool name if this is a [`ContentBlock::ToolUse`], otherwise `None`.
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            ContentBlock::ToolUse { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }

    /// Return the text content if this is a [`ContentBlock::Text`], otherwise `None`.
    pub fn text_content(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }
}

/// A user event from the transcript.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserEvent {
    /// UUID of this event.
    #[serde(default)]
    pub uuid: String,
    /// RFC 3339 timestamp string.
    #[serde(default)]
    pub timestamp: String,
    /// The user message payload.
    #[serde(default)]
    pub message: UserMessage,
}

/// The `message` field inside a user event.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserMessage {
    /// Content blocks (may include `tool_result` blocks).
    #[serde(default)]
    pub content: Vec<UserContentBlock>,
}

/// A content block in a user message.
///
/// The only block we care about is `tool_result`, which resolves pending tool calls.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum UserContentBlock {
    /// A tool result, resolving a prior `tool_use` request.
    #[serde(rename = "tool_result")]
    ToolResult {
        /// The `tool_use_id` from the matching assistant `tool_use` block.
        #[serde(default)]
        tool_use_id: String,
    },
    /// Any other user content block type — ignored.
    #[serde(other)]
    Other,
}

impl UserContentBlock {
    /// Return the resolved `tool_use_id` if this is a tool result, otherwise `None`.
    pub fn resolved_tool_use_id(&self) -> Option<&str> {
        match self {
            UserContentBlock::ToolResult { tool_use_id } => Some(tool_use_id.as_str()),
            _ => None,
        }
    }
}

/// A recent assistant message snapshot, used in the detail view.
///
/// This is derived data stored on the `TranscriptAggregates` struct (see
/// `telemetry::claude::parser`).
/// **It must never be serialized to JSON output** — it may contain message
/// text that the privacy contract prohibits from appearing in `--json` output.
#[derive(Debug, Clone)]
pub struct RecentMessage {
    /// RFC 3339 timestamp string from the event.
    pub timestamp: String,
    /// Model identifier.
    pub model: String,
    /// Stop reason (`"tool_use"`, `"end_turn"`, …).
    pub stop_reason: Option<String>,
    /// A short preview: first ~80 chars of text, or a tag like `[thinking]` / `[tool_use: Bash]`.
    pub preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_event(json: &str) -> TranscriptEvent {
        serde_json::from_str(json).expect("parse failed")
    }

    #[test]
    fn parses_assistant_event_with_usage() {
        let json = r#"{
            "type": "assistant",
            "uuid": "abc",
            "timestamp": "2026-04-21T10:00:00Z",
            "message": {
                "model": "claude-opus-4-7",
                "stop_reason": "tool_use",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 200,
                    "cache_creation_input_tokens": 50,
                    "cache_read_input_tokens": 300
                },
                "content": [
                    {"type": "tool_use", "id": "tu_001", "name": "Bash", "input": {}}
                ]
            }
        }"#;
        let ev = parse_event(json);
        match ev {
            TranscriptEvent::Assistant(a) => {
                assert_eq!(a.message.model, "claude-opus-4-7");
                assert_eq!(a.message.stop_reason.as_deref(), Some("tool_use"));
                assert_eq!(a.message.usage.input_tokens, 100);
                assert_eq!(a.message.usage.output_tokens, 200);
                assert_eq!(a.message.usage.cache_creation_input_tokens, 50);
                assert_eq!(a.message.usage.cache_read_input_tokens, 300);
                assert_eq!(a.message.content.len(), 1);
                let tool = &a.message.content[0];
                assert_eq!(tool.tool_name(), Some("Bash"));
                assert_eq!(tool.tool_use_id(), Some("tu_001"));
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn parses_user_event_with_tool_result() {
        let json = r#"{
            "type": "user",
            "uuid": "u1",
            "timestamp": "2026-04-21T10:00:01Z",
            "message": {
                "content": [
                    {"type": "tool_result", "tool_use_id": "tu_001", "content": "output text"}
                ]
            }
        }"#;
        let ev = parse_event(json);
        match ev {
            TranscriptEvent::User(u) => {
                assert_eq!(u.message.content.len(), 1);
                assert_eq!(u.message.content[0].resolved_tool_use_id(), Some("tu_001"));
            }
            other => panic!("expected User, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_type_becomes_other() {
        let json = r#"{"type": "file-history-snapshot", "data": {"foo": 1}}"#;
        let ev = parse_event(json);
        assert!(matches!(ev, TranscriptEvent::Other));
    }

    #[test]
    fn permission_mode_event_becomes_other() {
        let json = r#"{"type": "permission-mode", "value": "acceptEdits"}"#;
        let ev = parse_event(json);
        assert!(matches!(ev, TranscriptEvent::Other));
    }

    #[test]
    fn missing_usage_fields_default_to_zero() {
        let json = r#"{
            "type": "assistant",
            "message": {"model": "claude-sonnet-4-5", "content": []}
        }"#;
        let ev = parse_event(json);
        match ev {
            TranscriptEvent::Assistant(a) => {
                assert_eq!(a.message.usage.input_tokens, 0);
                assert_eq!(a.message.usage.output_tokens, 0);
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn assistant_text_content_block_parsed() {
        let json = r#"{
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": "Hello world"}]
            }
        }"#;
        let ev = parse_event(json);
        match ev {
            TranscriptEvent::Assistant(a) => {
                assert_eq!(a.message.content[0].text_content(), Some("Hello world"));
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn unknown_content_block_becomes_other() {
        let json = r#"{
            "type": "assistant",
            "message": {
                "content": [{"type": "server_tool_use", "id": "st1", "name": "WebSearch"}]
            }
        }"#;
        let ev = parse_event(json);
        match ev {
            TranscriptEvent::Assistant(a) => {
                assert!(matches!(a.message.content[0], ContentBlock::Other));
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn stop_reason_null_deserializes_to_none() {
        let json = r#"{
            "type": "assistant",
            "message": {"stop_reason": null, "content": []}
        }"#;
        let ev = parse_event(json);
        match ev {
            TranscriptEvent::Assistant(a) => {
                assert_eq!(a.message.stop_reason, None);
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }
}
