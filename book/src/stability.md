# Stability and compatibility

From **1.0.0** onwards agentop follows [Semantic Versioning](https://semver.org/).
Post-1.0 breaking changes bump the major version.

## Stable surfaces

These are covered by the version contract. A breaking change here requires
a major-version bump.

### CLI flags

Every flag documented in `agentop --help` and in the
[Installation](installation.md) / [Quickstart](quickstart.md) chapters. New
flags may be added at any time; existing flags retain their names and
semantics.

### stdout / stderr conventions

- `agentop --json` writes one line of JSON to stdout when
  `--pretty` is not set (NDJSON-compatible). With `--pretty`, indented JSON
- `agentop --list` writes a tabular text representation to stdout
- `agentop --generate-completions <shell>` writes a completion script to
  stdout
- The TUI writes only to the alternate screen; terminal scrollback is never
  modified

### `--json` schema

Currently **`schema_version = 1`**. The schema file at
[`docs/output-schema.md`](https://github.com/leboiko/claude-codex-pid-inspector/blob/master/docs/output-schema.md)
is the authoritative reference.

- **Additive changes** (new optional fields) do not bump `schema_version`
- **Breaking changes** (removing, renaming, or re-typing any field) bump
  `schema_version` to 2 (and beyond)
- **Every release documents its schema version** in the changelog

### Exit codes

- `0` — success
- non-zero — fatal error (process not launched, terminal init failed,
  argument parse failed)

### Config file

`$XDG_CONFIG_HOME/agentop/config.toml` with its documented keys
(`theme`, `graph_style`). New keys may be added; existing keys and their
accepted values are stable.

## Unstable surfaces

These may change without a major-version bump.

### TUI keybindings

Labelled unstable. We may remap keys to fit new features. The `?` help
overlay always shows the current bindings. If your muscle memory breaks
during a minor-version upgrade, the help overlay tells you what moved.

### Library API (`src/lib.rs`)

The binary is the product. The crate's library target exists only to share
types between `src/main.rs` and the integration tests. Nothing under
`agentop::` is part of the public API.

### Telemetry internals

- The **schema** of `AgentTelemetry` as serialised in `--json` is stable
- The **providers' heuristics** are not: status-inference rules, context
  fraction formulas, idle-tier thresholds, and pricing-table entries may
  evolve to track Claude / Codex behavioural changes

### Cognitive-decay score

Currently a context-saturation proxy. We may replace the formula with a
richer one as signals become available. The score bar will keep rendering
in the same place, but an arbitrary value of `37` today may correspond to
`52` under the new formula.

## Pricing audit cadence

The Claude and Codex pricing tables are pinned with `*_PRICING_AUDITED_AT`
constants:

- `CLAUDE_PRICING_AUDITED_AT` in `src/telemetry/claude/pricing.rs`
- `CODEX_PRICING_AUDITED_AT` in `src/telemetry/codex/pricing.rs`

We re-audit before every minor release. If a provider's pricing changes
between audits, cost estimates drift. Every `cost_usd` we emit carries
`cost_is_estimated: true` as an explicit acknowledgement.

## Platform support

- **macOS** (x86_64 and aarch64) — tier 1
- **Linux** (x86_64 and aarch64, gnu and musl) — tier 1
- **Windows** — not supported

The Homebrew tap, AUR package, and Nix flake all reflect this matrix.

## Reporting regressions

Open an issue at
<https://github.com/leboiko/claude-codex-pid-inspector/issues>. Include:

- `agentop --version`
- OS + kernel/darwin version
- Terminal emulator
- The offending command line or key sequence
- Whether `--json` schema was affected (if yes, tag with `schema`)
