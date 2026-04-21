//! Claude Code telemetry provider.
//!
//! This module implements the [`TelemetryProvider`](crate::telemetry::TelemetryProvider)
//! trait for Claude Code sessions by reading:
//!
//! - `~/.claude/sessions/{pid}.json` — session metadata (PID correlation).
//! - `~/.claude/projects/{cwd-slug}/{session_id}.jsonl` — transcript events.
//! - `/tmp/claude-{uid}/{cwd-slug}/{session_id}/tasks/` — subagent task files.
//!
//! # Submodules
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`pricing`] | Pricing table and cost computation |
//! | [`schema`]  | Serde types for session metadata and transcript events |
//! | [`parser`]  | Incremental JSONL transcript reader |
//! | [`discovery`] | Session index with TTL caching |
//! | [`subagents`] | Subagent task counting |
//! | [`health`] | Decay score formula |
//! | [`provider`] | [`ClaudeTelemetryProvider`] implementation |

pub mod discovery;
pub mod health;
pub mod parser;
pub mod pricing;
pub mod provider;
pub mod schema;
pub mod subagents;

pub use provider::ClaudeTelemetryProvider;
