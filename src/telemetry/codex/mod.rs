//! Codex CLI telemetry provider.
//!
//! This module implements the [`TelemetryProvider`](crate::telemetry::TelemetryProvider)
//! trait for OpenAI Codex CLI sessions by reading rollout files at:
//!
//! - `~/.codex/sessions/YYYY/MM/DD/rollout-{YYYY-MM-DDTHH-MM-SS}-{session-uuid}.jsonl`
//!
//! The default root `~/.codex/` can be overridden by setting the `CODEX_HOME`
//! environment variable on the Codex process. This module reads that variable
//! from the process environment (via `ps eww` on macOS or `/proc/{pid}/environ`
//! on Linux) to support non-default installations.
//!
//! # Design overview
//!
//! PID-to-rollout correlation is performed without a pidfile. The strategy is:
//!
//! 1. Resolve `CODEX_HOME` per PID by reading the process environment.
//! 2. Locate the rollout file whose filename timestamp is within ±90 seconds
//!    of the process start time.
//! 3. Tail the rollout file incrementally using a persisted byte offset.
//!
//! # Submodules
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`pricing`]   | Pricing table and cost computation |
//! | [`schema`]    | Serde types for rollout JSONL events |
//! | [`parser`]    | Incremental rollout file reader |
//! | [`discovery`] | CODEX_HOME resolution and rollout file location |
//! | [`provider`]  | [`CodexTelemetryProvider`] implementation |

pub mod discovery;
pub mod parser;
pub mod pricing;
pub mod provider;
pub mod schema;

pub use provider::CodexTelemetryProvider;
