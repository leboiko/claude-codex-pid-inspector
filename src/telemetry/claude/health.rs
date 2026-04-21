//! Health / decay score computation.
//!
//! The decay score is a 0–100 value indicating session "health". Higher values
//! mean more concern. The formula has four components:
//!
//! | Component             | Weight | Description |
//! |-----------------------|--------|-------------|
//! | `context_frac`        | 40     | Context window fill fraction × 40 |
//! | `error_accel`         | 25     | **Stubbed at 0 — Phase 2 follow-up** |
//! | `token_efficiency`    | 20     | **Stubbed at 0 — Phase 2 follow-up** |
//! | `repetition`          | 15     | **Stubbed at 0 — Phase 2 follow-up** |
//!
//! The three stubbed components require signals not yet available in Phase 2
//! (error rate trending, cache-read ratio drift, repetition detection).
//! They are documented here so the formula can be completed in a future phase
//! without changing the public API or the 0–100 range.
//!
//! Phase 3 / follow-up TODO:
//! - `error_accel`: track tool_result error rate over a sliding window.
//! - `token_efficiency`: compare `output_tokens / input_tokens` ratio trend.
//! - `repetition`: detect repeated tool calls or identical text blocks.

use super::parser::TranscriptAggregates;
use super::pricing::lookup as lookup_pricing;

/// Compute a 0–100 decay score for the given session aggregates.
///
/// The score increases as the context window fills up. The three other
/// components are stubbed at 0 for this phase.
///
/// # Arguments
///
/// * `agg`    - Accumulated transcript aggregates for the session.
/// * `window` - Optional override for the context window size in tokens.
///   When `None`, the window is looked up from the pricing table using
///   [`agg.model`](TranscriptAggregates::model).
pub fn decay_score(agg: &TranscriptAggregates, window: Option<u64>) -> u8 {
    let context_window = window.or_else(|| {
        agg.model
            .as_deref()
            .map(|m| lookup_pricing(m).context_window)
    });

    let context_frac_score: f64 = match (agg.context_tokens, context_window) {
        (Some(used), Some(total)) if total > 0 => {
            let frac = (used as f64 / total as f64).clamp(0.0, 1.0);
            frac * 40.0
        }
        _ => 0.0,
    };

    // TODO(Phase 3): implement error_accel (weight 25), token_efficiency (20),
    // repetition (15). For now they are always 0.
    let error_accel_score: f64 = 0.0;
    let token_efficiency_score: f64 = 0.0;
    let repetition_score: f64 = 0.0;

    let total = context_frac_score + error_accel_score + token_efficiency_score + repetition_score;
    total.round().clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_zero_when_no_context() {
        let agg = TranscriptAggregates::default();
        assert_eq!(decay_score(&agg, None), 0);
    }

    #[test]
    fn decay_zero_at_empty_context() {
        let agg = TranscriptAggregates {
            model: Some("claude-sonnet-4-5".to_string()),
            context_tokens: Some(0),
            ..Default::default()
        };
        assert_eq!(decay_score(&agg, Some(200_000)), 0);
    }

    #[test]
    fn decay_40_at_full_context() {
        let agg = TranscriptAggregates {
            model: Some("claude-sonnet-4-5".to_string()),
            context_tokens: Some(200_000),
            ..Default::default()
        };
        let score = decay_score(&agg, Some(200_000));
        assert_eq!(score, 40, "100% context fill should give score 40");
    }

    #[test]
    fn decay_20_at_half_context() {
        let agg = TranscriptAggregates {
            model: Some("claude-sonnet-4-5".to_string()),
            context_tokens: Some(100_000),
            ..Default::default()
        };
        let score = decay_score(&agg, Some(200_000));
        assert_eq!(score, 20, "50% context fill should give score 20");
    }

    #[test]
    fn decay_uses_model_window_when_override_is_none() {
        // opus-4-7 has a 1M context window.
        let agg = TranscriptAggregates {
            model: Some("claude-opus-4-7".to_string()),
            context_tokens: Some(500_000), // 50% of 1M
            ..Default::default()
        };
        let score = decay_score(&agg, None);
        assert_eq!(score, 20, "50% of 1M context should give score 20");
    }

    #[test]
    fn decay_clamped_to_100() {
        // Even if somehow context exceeds window, score must not exceed 100.
        let agg = TranscriptAggregates {
            model: Some("claude-sonnet-4-5".to_string()),
            context_tokens: Some(999_999_999),
            ..Default::default()
        };
        let score = decay_score(&agg, Some(200_000));
        assert!(score <= 100);
    }

    #[test]
    fn decay_score_with_window_override() {
        let agg = TranscriptAggregates {
            context_tokens: Some(50_000),
            ..Default::default()
        };
        // 50k / 100k = 50%, × 40 = 20
        let score = decay_score(&agg, Some(100_000));
        assert_eq!(score, 20);
    }
}
