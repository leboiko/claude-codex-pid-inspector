//! Telemetry-provider architecture for enriching process snapshots with
//! out-of-band agent data.
//!
//! # Overview
//!
//! The telemetry subsystem is structured as a **pipeline of providers**. On
//! every scanner tick the [`TelemetryPipeline`] runs each registered
//! [`TelemetryProvider`] in sequence, accumulating a [`TelemetryMap`] (a
//! `HashMap<u32, AgentTelemetry>` keyed by PID). The resulting map is shipped
//! back to [`crate::app::App`] alongside the regular process snapshot, where it
//! is stored and made available to UI renderers.
//!
//! # Providers
//!
//! - [`ClaudeTelemetryProvider`] — reads Claude Code session files and transcripts
//!   from `~/.claude/` (Phase 2).
//! - [`CodexTelemetryProvider`] — reads Codex CLI rollout files from `~/.codex/`
//!   (Phase 3). Each provider filters to its own [`ProcessKind`](crate::process::ProcessKind)
//!   so they can coexist in the same pipeline without stepping on each other.
//!
//! # Threading model
//!
//! [`TelemetryPipeline::enrich`] is called from the scanner's dedicated
//! `spawn_blocking` thread. Providers must implement [`Send`] but do not need
//! to be `Sync`; they are never shared across threads. See
//! [`TelemetryProvider`] for the full contract.

pub mod claude;
pub mod codex;
mod noop;
mod types;

pub use claude::ClaudeTelemetryProvider;
pub use codex::CodexTelemetryProvider;
pub use noop::NoopProvider;
pub use types::{AgentStatus, AgentTelemetry, TelemetryMap, TelemetryPipeline, TelemetryProvider};

// Re-export for UI detail view usage.
pub use claude::parser::TranscriptAggregates;
