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
//! # Phase 0 (this commit)
//!
//! The seam is fully wired but the only registered provider is [`NoopProvider`],
//! which does nothing. The [`TelemetryMap`] therefore always arrives empty and
//! no user-visible behaviour changes.
//!
//! # Future phases
//!
//! Phase 2 and Phase 3 will introduce concrete providers that read Claude and
//! Codex transcript files respectively, populate [`AgentTelemetry`] fields, and
//! infer [`AgentStatus`] from token counters and stop-reason signals.
//!
//! # Threading model
//!
//! [`TelemetryPipeline::enrich`] is called from the scanner's dedicated
//! `spawn_blocking` thread. Providers must implement [`Send`] but do not need
//! to be `Sync`; they are never shared across threads. See
//! [`TelemetryProvider`] for the full contract.

pub mod claude;
mod noop;
mod types;

pub use claude::ClaudeTelemetryProvider;
pub use noop::NoopProvider;
pub use types::{AgentStatus, AgentTelemetry, TelemetryMap, TelemetryPipeline, TelemetryProvider};

// Re-export for UI detail view usage.
pub use claude::parser::TranscriptAggregates;
