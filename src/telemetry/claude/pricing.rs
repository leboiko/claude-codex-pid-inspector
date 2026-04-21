//! Pricing table for Claude models.
//!
//! All rates are in USD per million tokens. The table is hardcoded and
//! represents best-known rates as of [`CLAUDE_PRICING_AUDITED_AT`]. A
//! top-of-file constant records the audit date so the team knows when to
//! re-check.
//!
//! Lookup uses longest-prefix matching over model identifiers so that
//! `"claude-opus-4-7"` (the 1M-context variant) resolves before the more
//! general `"claude-opus-4-"` pattern.

/// The date on which this pricing table was last verified against the
/// Anthropic pricing page. Used as a hint for future maintainers.
pub const CLAUDE_PRICING_AUDITED_AT: &str = "2026-04-21";

/// Per-model token rates (USD per million tokens) and context window size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pricing {
    /// USD per 1 million input tokens.
    pub input_per_m: f64,
    /// USD per 1 million output tokens.
    pub output_per_m: f64,
    /// USD per 1 million cache-write tokens.
    pub cache_write_per_m: f64,
    /// USD per 1 million cache-read tokens.
    pub cache_read_per_m: f64,
    /// Maximum context window in tokens for this model.
    pub context_window: u64,
}

/// Ordered pricing table. Earlier entries take precedence on prefix match.
///
/// **Order matters**: longer / more-specific prefixes must appear before
/// shorter ones. `"claude-opus-4-7"` must precede `"claude-opus-4-"`.
static PRICING_TABLE: &[(&str, Pricing)] = &[
    // claude-opus-4-7  (1M context)
    (
        "claude-opus-4-7",
        Pricing {
            input_per_m: 15.0,
            output_per_m: 75.0,
            cache_write_per_m: 18.75,
            cache_read_per_m: 1.50,
            context_window: 1_000_000,
        },
    ),
    // claude-opus-4-* (non-1M variants, 200k context)
    (
        "claude-opus-4-",
        Pricing {
            input_per_m: 15.0,
            output_per_m: 75.0,
            cache_write_per_m: 18.75,
            cache_read_per_m: 1.50,
            context_window: 200_000,
        },
    ),
    // claude-sonnet-4-*
    (
        "claude-sonnet-4-",
        Pricing {
            input_per_m: 3.0,
            output_per_m: 15.0,
            cache_write_per_m: 3.75,
            cache_read_per_m: 0.30,
            context_window: 200_000,
        },
    ),
    // claude-haiku-4-5*
    (
        "claude-haiku-4-5",
        Pricing {
            input_per_m: 1.0,
            output_per_m: 5.0,
            cache_write_per_m: 1.25,
            cache_read_per_m: 0.10,
            context_window: 200_000,
        },
    ),
];

/// Fallback pricing used for unknown model identifiers.
///
/// Uses Sonnet rates as a conservative estimate.
pub const FALLBACK_PRICING: Pricing = Pricing {
    input_per_m: 3.0,
    output_per_m: 15.0,
    cache_write_per_m: 3.75,
    cache_read_per_m: 0.30,
    context_window: 200_000,
};

/// Look up pricing for the given model identifier string.
///
/// Uses longest-prefix matching: iterates the pricing table in order and
/// returns the first entry whose prefix matches the start of `model`. Falls
/// back to Sonnet rates ([`FALLBACK_PRICING`]) if no prefix matches.
///
/// # Examples
///
/// ```
/// use agentop::telemetry::claude::pricing::lookup;
///
/// let p = lookup("claude-opus-4-7");
/// assert_eq!(p.context_window, 1_000_000);
///
/// let p2 = lookup("claude-sonnet-4-5");
/// assert_eq!(p2.input_per_m, 3.0);
///
/// // Unknown models fall back to Sonnet rates.
/// let p3 = lookup("claude-unknown-model");
/// assert_eq!(p3.input_per_m, 3.0);
/// ```
pub fn lookup(model: &str) -> Pricing {
    for (prefix, pricing) in PRICING_TABLE {
        if model.starts_with(prefix) {
            return *pricing;
        }
    }
    FALLBACK_PRICING
}

/// Compute the cost in USD for a single assistant turn.
///
/// # Arguments
///
/// * `pricing`       - Rate table for the model.
/// * `input`         - Input token count (excluding cache tokens).
/// * `output`        - Output token count.
/// * `cache_write`   - Cache-creation (write) token count.
/// * `cache_read`    - Cache-read token count.
pub fn compute_cost(
    pricing: &Pricing,
    input: u64,
    output: u64,
    cache_write: u64,
    cache_read: u64,
) -> f64 {
    (input as f64 / 1_000_000.0) * pricing.input_per_m
        + (output as f64 / 1_000_000.0) * pricing.output_per_m
        + (cache_write as f64 / 1_000_000.0) * pricing.cache_write_per_m
        + (cache_read as f64 / 1_000_000.0) * pricing.cache_read_per_m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_4_7_has_1m_context() {
        let p = lookup("claude-opus-4-7");
        assert_eq!(p.context_window, 1_000_000);
        assert_eq!(p.input_per_m, 15.0);
    }

    #[test]
    fn opus_4_7_matched_before_generic_opus_4() {
        // "claude-opus-4-7" should resolve to the specific 1M entry.
        let specific = lookup("claude-opus-4-7");
        assert_eq!(specific.context_window, 1_000_000);
        // A different opus-4 variant resolves to the generic 200k entry.
        let generic = lookup("claude-opus-4-5");
        assert_eq!(generic.context_window, 200_000);
    }

    #[test]
    fn sonnet_4_resolves_correctly() {
        let p = lookup("claude-sonnet-4-5");
        assert_eq!(p.input_per_m, 3.0);
        assert_eq!(p.output_per_m, 15.0);
        assert_eq!(p.context_window, 200_000);
    }

    #[test]
    fn haiku_4_5_resolves_correctly() {
        let p = lookup("claude-haiku-4-5");
        assert_eq!(p.input_per_m, 1.0);
        assert_eq!(p.output_per_m, 5.0);
    }

    #[test]
    fn unknown_model_falls_back_to_sonnet_rates() {
        let p = lookup("claude-totally-new-model");
        assert_eq!(p.input_per_m, 3.0);
        assert_eq!(p.output_per_m, 15.0);
    }

    #[test]
    fn empty_model_string_falls_back() {
        let p = lookup("");
        assert_eq!(p.input_per_m, FALLBACK_PRICING.input_per_m);
    }

    #[test]
    fn compute_cost_zero_tokens() {
        let p = lookup("claude-sonnet-4-5");
        let cost = compute_cost(&p, 0, 0, 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn compute_cost_one_million_input_tokens() {
        let p = lookup("claude-sonnet-4-5");
        // 1M input tokens at $3/M = $3.00
        let cost = compute_cost(&p, 1_000_000, 0, 0, 0);
        assert!((cost - 3.0).abs() < 1e-9, "expected $3.00, got {cost}");
    }

    #[test]
    fn compute_cost_all_token_types() {
        let p = lookup("claude-opus-4-7");
        // 1M each: $15 input + $75 output + $18.75 cache_write + $1.50 cache_read = $110.25
        let cost = compute_cost(&p, 1_000_000, 1_000_000, 1_000_000, 1_000_000);
        assert!((cost - 110.25).abs() < 1e-6, "expected $110.25, got {cost}");
    }
}
