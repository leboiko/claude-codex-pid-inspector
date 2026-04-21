//! No-op telemetry provider used during Phase 0.
//!
//! [`NoopProvider`] satisfies the [`TelemetryProvider`] trait without doing
//! anything. It is registered as the default provider until Phase 2/3
//! introduce concrete transcript-based implementations.

use crate::process::ProcessInfo;

use super::types::{TelemetryMap, TelemetryProvider};

/// A [`TelemetryProvider`] that never writes any telemetry.
///
/// Used as a stand-in until real providers are wired in. Registering it
/// keeps the pipeline infrastructure exercised end-to-end even when no
/// transcript data is available.
pub struct NoopProvider;

impl TelemetryProvider for NoopProvider {
    fn name(&self) -> &'static str {
        "noop"
    }

    /// Does nothing — leaves `out` unchanged.
    fn enrich(&mut self, _processes: &[ProcessInfo], _out: &mut TelemetryMap) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessInfo;

    fn make_proc(pid: u32) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: None,
            name: "test".to_string(),
            cmd: vec!["test".to_string()],
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

    #[test]
    fn noop_produces_empty_map() {
        let mut provider = NoopProvider;
        let processes = vec![make_proc(1), make_proc(2)];
        let mut out = TelemetryMap::new();
        provider.enrich(&processes, &mut out);
        assert!(out.is_empty(), "NoopProvider must not write any entries");
    }

    #[test]
    fn noop_does_not_overwrite_existing_entries() {
        use crate::telemetry::types::AgentTelemetry;

        let mut provider = NoopProvider;
        let processes = vec![make_proc(42)];
        let mut out = TelemetryMap::new();

        // Pre-populate an entry; NoopProvider must leave it untouched.
        let existing = AgentTelemetry {
            session_id: Some("existing".to_string()),
            ..Default::default()
        };
        out.insert(42, existing.clone());

        provider.enrich(&processes, &mut out);

        assert_eq!(out.get(&42), Some(&existing));
    }
}
