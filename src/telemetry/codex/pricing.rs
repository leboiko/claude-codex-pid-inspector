//! Pricing table for Codex / OpenAI models.
//!
//! # Important disclaimer
//!
//! **All rates in this table are estimates.** OpenAI does not publish per-model
//! token pricing for Codex sessions in the rollout file format; the figures
//! below are derived from publicly observable list prices as of the audit date
//! and from inferences based on the model tier. Every entry is annotated with
//! its source. Consumers should treat `cost_usd` as an *order-of-magnitude*
//! signal, not an invoice-accurate figure.
//!
//! The audit date constant [`CODEX_PRICING_AUDITED_AT`] records when the table
//! was last reviewed so future maintainers know when to re-check.
//!
//! # Lookup
//!
//! [`lookup`] uses longest-prefix matching over model identifiers so that
//! `"gpt-5-codex"` (a hypothetical specific Codex variant) would resolve
//! before the more general `"gpt-5"` pattern.

/// The date on which this pricing table was last reviewed against public
/// OpenAI pricing pages.
pub const CODEX_PRICING_AUDITED_AT: &str = "2026-04-21";

/// Per-model token rates (USD per million tokens).
///
/// All rates are **estimates** — see the module-level disclaimer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodexPricing {
    /// Estimated USD per 1 million input tokens.
    pub input_per_m: f64,
    /// Estimated USD per 1 million output tokens (includes reasoning tokens).
    pub output_per_m: f64,
    /// Estimated USD per 1 million cached input tokens (typically ~10% of input rate).
    pub cached_input_per_m: f64,
}

/// Ordered pricing table. Earlier entries take precedence on prefix match.
///
/// **Order matters**: longer / more-specific prefixes must appear before
/// shorter ones. `"gpt-5-codex"` must precede `"gpt-5"`.
///
/// # Sources
///
/// - `gpt-5*` / `gpt-5-codex`: Estimated ~$1.25/$10 per million input/output,
///   aligning with the gpt-4.1 tier as of 2026-04-21 (openai.com/pricing).
///   No Codex-specific pricing has been published separately.
/// - `o3*` / `o4*` reasoning models: same flat rates used for a first cut;
///   reasoning tokens are billed at the output rate per OpenAI policy.
/// - Cached input: ~10% of standard input rate, following OpenAI's prompt-
///   caching discount pattern.
static PRICING_TABLE: &[(&str, CodexPricing)] = &[
    // gpt-5-codex — specific Codex variant; same estimate as gpt-5 tier
    // Source: estimate aligned with gpt-4.1 tier as of 2026-04-21
    (
        "gpt-5-codex",
        CodexPricing {
            input_per_m: 1.25,
            output_per_m: 10.0,
            cached_input_per_m: 0.125,
        },
    ),
    // gpt-5* family
    // Source: estimate aligned with gpt-4.1 tier as of 2026-04-21
    (
        "gpt-5",
        CodexPricing {
            input_per_m: 1.25,
            output_per_m: 10.0,
            cached_input_per_m: 0.125,
        },
    ),
    // o4-mini — reasoning model; output billed at full output rate
    // Source: openai.com/pricing as of 2026-04-21 ($1.10/$4.40 per M)
    (
        "o4-mini",
        CodexPricing {
            input_per_m: 1.10,
            output_per_m: 4.40,
            cached_input_per_m: 0.275,
        },
    ),
    // o4* reasoning models — same broad tier
    // Source: estimate, reasoning tokens billed at output rate per OpenAI policy
    (
        "o4",
        CodexPricing {
            input_per_m: 2.0,
            output_per_m: 8.0,
            cached_input_per_m: 0.50,
        },
    ),
    // o3-mini — reasoning model
    // Source: openai.com/pricing as of 2026-04-21 ($1.10/$4.40 per M)
    (
        "o3-mini",
        CodexPricing {
            input_per_m: 1.10,
            output_per_m: 4.40,
            cached_input_per_m: 0.275,
        },
    ),
    // o3* reasoning models
    // Source: openai.com/pricing as of 2026-04-21 ($10/$40 per M)
    (
        "o3",
        CodexPricing {
            input_per_m: 10.0,
            output_per_m: 40.0,
            cached_input_per_m: 2.50,
        },
    ),
    // gpt-4.1 family
    // Source: openai.com/pricing as of 2026-04-21 ($2/$8 per M)
    (
        "gpt-4.1",
        CodexPricing {
            input_per_m: 2.0,
            output_per_m: 8.0,
            cached_input_per_m: 0.50,
        },
    ),
    // gpt-4o family
    // Source: openai.com/pricing as of 2026-04-21 ($2.50/$10 per M)
    (
        "gpt-4o",
        CodexPricing {
            input_per_m: 2.50,
            output_per_m: 10.0,
            cached_input_per_m: 1.25,
        },
    ),
    // gpt-4* fallback
    // Source: conservative estimate for unknown gpt-4 variants
    (
        "gpt-4",
        CodexPricing {
            input_per_m: 2.50,
            output_per_m: 10.0,
            cached_input_per_m: 1.25,
        },
    ),
];

/// Fallback pricing used for unknown model identifiers.
///
/// Uses gpt-5 / gpt-4.1 rates as a conservative mid-tier estimate.
/// All values are **estimates**.
pub const FALLBACK_PRICING: CodexPricing = CodexPricing {
    input_per_m: 2.0,
    output_per_m: 8.0,
    cached_input_per_m: 0.50,
};

/// Look up pricing for the given model identifier string.
///
/// Uses longest-prefix matching: iterates the pricing table in order and
/// returns the first entry whose prefix is a prefix of `model`. Falls
/// back to [`FALLBACK_PRICING`] if no prefix matches.
///
/// All returned rates are **estimates** — see the module-level disclaimer.
///
/// # Examples
///
/// ```
/// use agentop::telemetry::codex::pricing::lookup;
///
/// let p = lookup("gpt-5.4");
/// assert_eq!(p.output_per_m, 10.0);
///
/// let p2 = lookup("o3-mini");
/// assert_eq!(p2.input_per_m, 1.10);
///
/// // Unknown models fall back to mid-tier rates.
/// let p3 = lookup("unknown-model-xyz");
/// assert_eq!(p3.output_per_m, 8.0);
/// ```
pub fn lookup(model: &str) -> CodexPricing {
    for (prefix, pricing) in PRICING_TABLE {
        if model.starts_with(prefix) {
            return *pricing;
        }
    }
    FALLBACK_PRICING
}

/// Compute the estimated cost in USD for cumulative token counts.
///
/// Reasoning tokens (`reasoning_output`) are billed at the standard output
/// rate per OpenAI policy — they are not separately priced.
///
/// # Arguments
///
/// * `pricing`        - Rate table for the model.
/// * `input`          - Cumulative non-cached input tokens.
/// * `cached_input`   - Cumulative cached input tokens.
/// * `output`         - Cumulative output tokens (excluding reasoning).
/// * `reasoning`      - Cumulative reasoning output tokens.
pub fn compute_cost(
    pricing: &CodexPricing,
    input: u64,
    cached_input: u64,
    output: u64,
    reasoning: u64,
) -> f64 {
    let total_output = output + reasoning;
    (input as f64 / 1_000_000.0) * pricing.input_per_m
        + (cached_input as f64 / 1_000_000.0) * pricing.cached_input_per_m
        + (total_output as f64 / 1_000_000.0) * pricing.output_per_m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpt5_resolves_correctly() {
        let p = lookup("gpt-5.4");
        assert_eq!(p.input_per_m, 1.25);
        assert_eq!(p.output_per_m, 10.0);
    }

    #[test]
    fn gpt5_codex_resolved_before_gpt5() {
        let specific = lookup("gpt-5-codex");
        let generic = lookup("gpt-5.4");
        // Both have the same rates currently, but specific prefix matches first
        assert_eq!(specific.input_per_m, generic.input_per_m);
    }

    #[test]
    fn o3_resolves_correctly() {
        let p = lookup("o3");
        assert_eq!(p.input_per_m, 10.0);
        assert_eq!(p.output_per_m, 40.0);
    }

    #[test]
    fn o3_mini_resolves_before_o3() {
        let mini = lookup("o3-mini");
        let full = lookup("o3");
        assert!(mini.output_per_m < full.output_per_m);
    }

    #[test]
    fn o4_mini_resolves_before_o4() {
        let mini = lookup("o4-mini");
        let full = lookup("o4");
        assert!(mini.output_per_m < full.output_per_m);
    }

    #[test]
    fn gpt4_1_resolves_correctly() {
        let p = lookup("gpt-4.1");
        assert_eq!(p.input_per_m, 2.0);
        assert_eq!(p.output_per_m, 8.0);
    }

    #[test]
    fn unknown_model_falls_back() {
        let p = lookup("unknown-model-xyz");
        assert_eq!(p.input_per_m, FALLBACK_PRICING.input_per_m);
        assert_eq!(p.output_per_m, FALLBACK_PRICING.output_per_m);
    }

    #[test]
    fn empty_model_string_falls_back() {
        let p = lookup("");
        assert_eq!(p.input_per_m, FALLBACK_PRICING.input_per_m);
    }

    #[test]
    fn compute_cost_zero_tokens() {
        let p = lookup("gpt-5.4");
        let cost = compute_cost(&p, 0, 0, 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn compute_cost_one_million_output_tokens() {
        let p = lookup("gpt-5.4"); // $10/M output
        let cost = compute_cost(&p, 0, 0, 1_000_000, 0);
        assert!((cost - 10.0).abs() < 1e-9, "expected $10.00, got {cost}");
    }

    #[test]
    fn compute_cost_reasoning_billed_at_output_rate() {
        let p = lookup("gpt-5.4"); // $10/M output
        let cost_pure = compute_cost(&p, 0, 0, 1_000_000, 0);
        let cost_reasoning = compute_cost(&p, 0, 0, 0, 1_000_000);
        // Both should cost the same: reasoning billed at output rate.
        assert!((cost_pure - cost_reasoning).abs() < 1e-9);
    }

    #[test]
    fn compute_cost_cached_cheaper_than_full_input() {
        let p = lookup("gpt-5.4");
        let full_cost = compute_cost(&p, 1_000_000, 0, 0, 0);
        let cached_cost = compute_cost(&p, 0, 1_000_000, 0, 0);
        assert!(cached_cost < full_cost, "cached input should be cheaper");
    }
}
