//! Core telemetry types shared across all providers.
//!
//! These types form the data contract between telemetry providers and the rest
//! of the application. They are intentionally provider-agnostic — providers
//! populate them; the UI reads them.

use std::collections::HashMap;

use crate::process::ProcessInfo;

/// Inferred agent status, derived from telemetry (tokens, pending tool, stop
/// reason) and CPU. Populated by [`TelemetryProvider`]s during enrichment.
/// Non-agent processes never get one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentStatus {
    /// The provider could not determine a status.
    #[default]
    Unknown,
    /// Awaiting user approval for a tool call.
    NeedsInput,
    /// Actively generating or executing.
    Processing,
    /// Waiting for the next user prompt (explicit `end_turn`).
    WaitingInput,
    /// More than 10 minutes since last activity.
    Idle,
    /// Process exited.
    Finished,
}

/// Enriched per-agent telemetry collected from sources outside `ps`/sysinfo —
/// typically a Claude/Codex transcript file. Fields are `Option` because a
/// provider may only have partial information.
///
/// Agent processes that have no provider, or whose provider couldn't locate
/// their transcript, get a `Default` instance (all `None`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentTelemetry {
    /// Unique identifier for the session, if available.
    pub session_id: Option<String>,
    /// Human-readable session name, if available.
    pub session_name: Option<String>,
    /// Model identifier (e.g. `"claude-opus-4-5"`), if known.
    pub model: Option<String>,

    /// Cumulative input token count for the session.
    pub input_tokens: Option<u64>,
    /// Cumulative output token count for the session.
    pub output_tokens: Option<u64>,
    /// Cumulative cache-read token count for the session.
    pub cache_read_tokens: Option<u64>,
    /// Cumulative cache-write token count for the session.
    pub cache_write_tokens: Option<u64>,
    /// Current number of tokens in the context window.
    pub context_tokens: Option<u64>,
    /// Maximum context window size for the active model.
    pub context_window: Option<u64>,

    /// Estimated or exact session cost in USD.
    ///
    /// `cost_is_estimated = true` means the value was computed from a local
    /// pricing table rather than read directly from the transcript.
    pub cost_usd: Option<f64>,
    /// `true` when [`cost_usd`](AgentTelemetry::cost_usd) was derived from a
    /// local pricing table rather than the transcript.
    pub cost_is_estimated: bool,

    /// The tool call currently awaiting user approval, if any.
    pub pending_tool: Option<String>,
    /// The most recent model stop reason observed (e.g. `"end_turn"`, `"tool_use"`).
    pub last_stop_reason: Option<String>,

    /// Number of in-flight subagent tasks spawned by this agent.
    pub subagent_count: usize,

    /// Inferred activity status. `None` means the provider did not compute one.
    pub status: Option<AgentStatus>,
}

impl AgentTelemetry {
    /// Returns `true` when at least one field has a value beyond
    /// [`Default::default()`].
    ///
    /// Useful for quickly distinguishing enriched entries from placeholder
    /// `Default` instances that were inserted only because a PID was seen in the
    /// process list.
    pub fn has_data(&self) -> bool {
        self.session_id.is_some()
            || self.session_name.is_some()
            || self.model.is_some()
            || self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.cache_read_tokens.is_some()
            || self.cache_write_tokens.is_some()
            || self.context_tokens.is_some()
            || self.context_window.is_some()
            || self.cost_usd.is_some()
            || self.cost_is_estimated
            || self.pending_tool.is_some()
            || self.last_stop_reason.is_some()
            || self.subagent_count > 0
            || self.status.is_some()
    }

    /// Returns the fraction of the context window currently consumed, or
    /// `None` if either [`context_tokens`](AgentTelemetry::context_tokens) or
    /// [`context_window`](AgentTelemetry::context_window) is missing.
    ///
    /// The result is clamped to `[0.0, 1.0]` to guard against values that
    /// exceed the nominal window size (e.g. due to counting methodology
    /// differences across providers).
    pub fn context_fraction(&self) -> Option<f32> {
        let used = self.context_tokens? as f32;
        let total = self.context_window? as f32;
        if total == 0.0 {
            return None;
        }
        Some((used / total).clamp(0.0, 1.0))
    }
}

/// Per-tick telemetry keyed by PID.
///
/// This map is cheap to clone — it is rebuilt from scratch on every scanner
/// tick by running the [`TelemetryPipeline`].
pub type TelemetryMap = HashMap<u32, AgentTelemetry>;

/// Enriches a snapshot of processes with out-of-band telemetry.
///
/// Providers are stateful: they may cache file handles, byte offsets, session
/// ID resolutions, etc. across ticks. [`enrich`](TelemetryProvider::enrich) is
/// called on every refresh with the current process snapshot; the provider may
/// modify `out` in place to record telemetry for PIDs it recognises.
///
/// # Threading model
///
/// Providers run on the scanner's dedicated blocking thread (spawned via
/// [`tokio::task::spawn_blocking`]). They must implement [`Send`] so they can
/// be moved onto that thread; they do NOT need to be `Sync`. There is exactly
/// one provider instance per pipeline, so no interior mutability or locking is
/// required.
///
/// # Error-handling contract
///
/// Providers **MUST NOT panic** and **MUST NOT block for more than a few
/// hundred milliseconds**. Errors should degrade gracefully: produce partial or
/// empty telemetry for the affected PIDs, log a diagnostic if useful, then
/// return. A provider that cannot read its transcript should leave `out`
/// unchanged for those PIDs rather than failing the whole tick.
pub trait TelemetryProvider: Send {
    /// A stable, human-readable identifier used in logs and the debug status bar.
    ///
    /// Must be a `'static` string (typically a string literal).
    fn name(&self) -> &'static str;

    /// Called once per scanner tick with the full process snapshot.
    ///
    /// The provider is responsible for filtering `processes` to the PIDs it
    /// cares about and writing the resulting [`AgentTelemetry`] values into
    /// `out`. It may overwrite entries written by an earlier provider if it has
    /// higher-fidelity data.
    ///
    /// # Arguments
    ///
    /// * `processes` — Full flat list of process snapshots from the current
    ///   refresh cycle.
    /// * `out` — Map to write enriched telemetry into, keyed by PID.
    fn enrich(&mut self, processes: &[ProcessInfo], out: &mut TelemetryMap);
}

/// Runs a fixed sequence of [`TelemetryProvider`]s in order.
///
/// The final [`TelemetryMap`] is the union of all provider outputs. Later
/// providers can override earlier entries for the same PID if they have
/// higher-fidelity data, though in practice each provider handles a disjoint
/// set of process kinds.
pub struct TelemetryPipeline {
    providers: Vec<Box<dyn TelemetryProvider>>,
}

impl TelemetryPipeline {
    /// Create an empty pipeline with no providers registered.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Append a provider to the pipeline, returning `self` for chaining.
    ///
    /// Providers run in registration order; later providers may override
    /// entries written by earlier ones.
    pub fn with_provider(mut self, p: Box<dyn TelemetryProvider>) -> Self {
        self.providers.push(p);
        self
    }

    /// Run all providers against `processes` and return the merged
    /// [`TelemetryMap`].
    ///
    /// Each provider receives the same `processes` slice and the same
    /// accumulating `out` map. The map starts empty and grows as providers
    /// contribute entries.
    pub fn enrich(&mut self, processes: &[ProcessInfo]) -> TelemetryMap {
        let mut out = TelemetryMap::new();
        for provider in &mut self.providers {
            provider.enrich(processes, &mut out);
        }
        out
    }

    /// Returns the [`name()`](TelemetryProvider::name) of every registered
    /// provider, in registration order.
    pub fn provider_names(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }
}

impl Default for TelemetryPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AgentTelemetry::has_data ─────────────────────────────────────────────

    #[test]
    fn has_data_default_is_false() {
        assert!(!AgentTelemetry::default().has_data());
    }

    #[test]
    fn has_data_session_id() {
        let t = AgentTelemetry {
            session_id: Some("s".to_string()),
            ..Default::default()
        };
        assert!(t.has_data());
    }

    #[test]
    fn has_data_subagent_count() {
        let t = AgentTelemetry {
            subagent_count: 1,
            ..Default::default()
        };
        assert!(t.has_data());
    }

    #[test]
    fn has_data_cost_is_estimated_flag() {
        let t = AgentTelemetry {
            cost_is_estimated: true,
            ..Default::default()
        };
        assert!(t.has_data());
    }

    #[test]
    fn has_data_status() {
        let t = AgentTelemetry {
            status: Some(AgentStatus::Processing),
            ..Default::default()
        };
        assert!(t.has_data());
    }

    // ── AgentTelemetry::context_fraction ────────────────────────────────────

    #[test]
    fn context_fraction_none_when_missing_tokens() {
        let t = AgentTelemetry {
            context_window: Some(1000),
            ..Default::default()
        };
        assert_eq!(t.context_fraction(), None);
    }

    #[test]
    fn context_fraction_none_when_missing_window() {
        let t = AgentTelemetry {
            context_tokens: Some(500),
            ..Default::default()
        };
        assert_eq!(t.context_fraction(), None);
    }

    #[test]
    fn context_fraction_none_when_both_missing() {
        assert_eq!(AgentTelemetry::default().context_fraction(), None);
    }

    #[test]
    fn context_fraction_correct_value() {
        let t = AgentTelemetry {
            context_tokens: Some(250),
            context_window: Some(1000),
            ..Default::default()
        };
        let frac = t.context_fraction().expect("should have a fraction");
        assert!((frac - 0.25).abs() < 1e-6, "expected 0.25, got {frac}");
    }

    #[test]
    fn context_fraction_clamped_above_one() {
        // Tokens exceed nominal window — must clamp to 1.0.
        let t = AgentTelemetry {
            context_tokens: Some(1200),
            context_window: Some(1000),
            ..Default::default()
        };
        let frac = t.context_fraction().expect("should have a fraction");
        assert!(
            (frac - 1.0).abs() < 1e-6,
            "expected 1.0 (clamped), got {frac}"
        );
    }

    #[test]
    fn context_fraction_zero_window_returns_none() {
        let t = AgentTelemetry {
            context_tokens: Some(0),
            context_window: Some(0),
            ..Default::default()
        };
        assert_eq!(t.context_fraction(), None);
    }
}
