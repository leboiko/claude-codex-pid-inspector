//! Integration tests for the Phase 0 telemetry-provider seam.
//!
//! Verifies that:
//! - An empty pipeline returns an empty map.
//! - [`NoopProvider`] produces an empty map.
//! - A single [`TestProvider`] carries its data through the pipeline.
//! - Multiple providers each contribute, and a later provider can override
//!   earlier data for the same PID.

mod common;

use agentop::telemetry::{AgentStatus, AgentTelemetry, NoopProvider, TelemetryPipeline};

use common::{make_process_info, TestProvider};

// ── Empty pipeline ───────────────────────────────────────────────────────────

#[test]
fn empty_pipeline_returns_empty_map() {
    let processes = vec![
        make_process_info(1, None, "claude"),
        make_process_info(2, Some(1), "node"),
    ];
    let mut pipeline = TelemetryPipeline::new();
    let map = pipeline.enrich(&processes);
    assert!(
        map.is_empty(),
        "a pipeline with no providers must return an empty map"
    );
}

#[test]
fn empty_pipeline_provider_names_is_empty() {
    let pipeline = TelemetryPipeline::new();
    assert!(pipeline.provider_names().is_empty());
}

// ── NoopProvider ─────────────────────────────────────────────────────────────

#[test]
fn noop_provider_produces_empty_map() {
    let processes = vec![
        make_process_info(10, None, "claude"),
        make_process_info(11, Some(10), "node"),
    ];
    let mut pipeline = TelemetryPipeline::new().with_provider(Box::new(NoopProvider));
    let map = pipeline.enrich(&processes);
    assert!(
        map.is_empty(),
        "NoopProvider must not contribute any telemetry entries"
    );
}

#[test]
fn noop_provider_name_is_correct() {
    let pipeline = TelemetryPipeline::new().with_provider(Box::new(NoopProvider));
    assert_eq!(pipeline.provider_names(), vec!["noop"]);
}

// ── Single TestProvider ───────────────────────────────────────────────────────

#[test]
fn single_provider_carries_data_through_pipeline() {
    let expected = AgentTelemetry {
        session_id: Some("sess-abc".to_string()),
        model: Some("claude-opus-4-5".to_string()),
        input_tokens: Some(1024),
        output_tokens: Some(512),
        status: Some(AgentStatus::Processing),
        ..Default::default()
    };

    let processes = vec![make_process_info(42, None, "claude")];
    let provider = TestProvider::new("test-claude", [(42, expected.clone())]);
    let mut pipeline = TelemetryPipeline::new().with_provider(Box::new(provider));

    let map = pipeline.enrich(&processes);

    assert_eq!(map.len(), 1, "expected exactly one entry");
    let got = map.get(&42).expect("PID 42 should be in the map");
    assert_eq!(got, &expected);
}

#[test]
fn provider_for_absent_pid_still_writes_to_map() {
    // The provider is not constrained to PIDs present in `processes` — it may
    // write entries for any PID it knows about.
    let expected = AgentTelemetry {
        session_id: Some("orphan".to_string()),
        ..Default::default()
    };
    let processes = vec![make_process_info(1, None, "bash")]; // PID 99 is absent
    let provider = TestProvider::new("test", [(99, expected.clone())]);
    let mut pipeline = TelemetryPipeline::new().with_provider(Box::new(provider));

    let map = pipeline.enrich(&processes);
    assert_eq!(map.get(&99), Some(&expected));
}

// ── Multiple providers ────────────────────────────────────────────────────────

#[test]
fn multiple_providers_each_contribute_disjoint_pids() {
    let telemetry_a = AgentTelemetry {
        session_id: Some("provider-a".to_string()),
        ..Default::default()
    };
    let telemetry_b = AgentTelemetry {
        session_id: Some("provider-b".to_string()),
        ..Default::default()
    };

    let processes = vec![
        make_process_info(10, None, "claude"),
        make_process_info(20, None, "codex"),
    ];

    let provider_a = TestProvider::new("provider-a", [(10, telemetry_a.clone())]);
    let provider_b = TestProvider::new("provider-b", [(20, telemetry_b.clone())]);

    let mut pipeline = TelemetryPipeline::new()
        .with_provider(Box::new(provider_a))
        .with_provider(Box::new(provider_b));

    let map = pipeline.enrich(&processes);

    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&10), Some(&telemetry_a));
    assert_eq!(map.get(&20), Some(&telemetry_b));
}

#[test]
fn later_provider_overrides_earlier_provider_for_same_pid() {
    let first = AgentTelemetry {
        session_id: Some("first".to_string()),
        model: Some("old-model".to_string()),
        input_tokens: Some(100),
        ..Default::default()
    };
    let second = AgentTelemetry {
        session_id: Some("second".to_string()),
        model: Some("new-model".to_string()),
        input_tokens: Some(999),
        status: Some(AgentStatus::WaitingInput),
        ..Default::default()
    };

    let processes = vec![make_process_info(7, None, "claude")];

    let provider_first = TestProvider::new("first", [(7, first)]);
    let provider_second = TestProvider::new("second", [(7, second.clone())]);

    let mut pipeline = TelemetryPipeline::new()
        .with_provider(Box::new(provider_first))
        .with_provider(Box::new(provider_second));

    let map = pipeline.enrich(&processes);

    assert_eq!(map.len(), 1, "same PID must not duplicate");
    assert_eq!(
        map.get(&7),
        Some(&second),
        "later provider must override earlier data"
    );
}

#[test]
fn provider_names_reported_in_registration_order() {
    let pipeline = TelemetryPipeline::new()
        .with_provider(Box::new(TestProvider::new("alpha", [])))
        .with_provider(Box::new(NoopProvider))
        .with_provider(Box::new(TestProvider::new("gamma", [])));

    assert_eq!(pipeline.provider_names(), vec!["alpha", "noop", "gamma"]);
}

// ── Pipeline with noop mixed alongside real providers ────────────────────────

#[test]
fn noop_does_not_erase_data_written_by_earlier_provider() {
    let expected = AgentTelemetry {
        session_id: Some("preserved".to_string()),
        ..Default::default()
    };
    let processes = vec![make_process_info(5, None, "claude")];

    let provider = TestProvider::new("real", [(5, expected.clone())]);
    let mut pipeline = TelemetryPipeline::new()
        .with_provider(Box::new(provider))
        .with_provider(Box::new(NoopProvider)); // runs after — must not clobber

    let map = pipeline.enrich(&processes);

    assert_eq!(map.get(&5), Some(&expected));
}

// ── AgentTelemetry::has_data (integration-level smoke) ───────────────────────

#[test]
fn has_data_false_for_default() {
    assert!(!AgentTelemetry::default().has_data());
}

#[test]
fn has_data_true_after_enrichment() {
    let t = AgentTelemetry {
        input_tokens: Some(1),
        ..Default::default()
    };
    assert!(t.has_data());
}
