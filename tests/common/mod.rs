//! Shared test helpers for integration tests.
//!
//! Provides a [`make_process_info`] factory and a configurable [`TestProvider`]
//! that can be reused across test files in future phases.

use std::collections::HashMap;

use agentop::process::ProcessInfo;
use agentop::telemetry::{AgentTelemetry, TelemetryMap, TelemetryProvider};

/// Build a minimal [`ProcessInfo`] suitable for use in tests.
///
/// # Arguments
///
/// * `pid`    — Numeric process identifier.
/// * `parent` — Parent PID, or `None` for a root process.
/// * `name`   — Short executable name.
pub fn make_process_info(pid: u32, parent: Option<u32>, name: &str) -> ProcessInfo {
    ProcessInfo {
        pid,
        parent_pid: parent,
        name: name.to_string(),
        cmd: vec![name.to_string()],
        exe_path: None,
        cwd: None,
        cpu_usage: 0.0,
        memory_bytes: 0,
        status: "Run".to_string(),
        environ_count: 0,
        start_time: 0,
        run_time: 0,
    }
}

/// A test double for [`TelemetryProvider`] that writes a fixed, pre-configured
/// set of telemetry entries.
///
/// Construct it with a map of `pid → AgentTelemetry`, then register it in a
/// [`agentop::telemetry::TelemetryPipeline`]. Each call to [`enrich`] merges
/// the fixed map into `out`, overwriting any existing entries for matching PIDs.
///
/// # Example
///
/// ```rust,ignore
/// let provider = TestProvider::new([(42, AgentTelemetry {
///     session_id: Some("abc".to_string()),
///     ..Default::default()
/// })]);
/// let mut pipeline = TelemetryPipeline::new().with_provider(Box::new(provider));
/// let map = pipeline.enrich(&processes);
/// assert!(map.contains_key(&42));
/// ```
pub struct TestProvider {
    name: &'static str,
    entries: HashMap<u32, AgentTelemetry>,
}

impl TestProvider {
    /// Create a [`TestProvider`] from any iterator of `(pid, AgentTelemetry)` pairs.
    pub fn new(
        name: &'static str,
        entries: impl IntoIterator<Item = (u32, AgentTelemetry)>,
    ) -> Self {
        Self {
            name,
            entries: entries.into_iter().collect(),
        }
    }
}

impl TelemetryProvider for TestProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn enrich(&mut self, _processes: &[ProcessInfo], out: &mut TelemetryMap) {
        for (&pid, telemetry) in &self.entries {
            out.insert(pid, telemetry.clone());
        }
    }
}
