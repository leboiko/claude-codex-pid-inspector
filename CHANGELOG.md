# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--list` flag: prints a plain-text process table (PID, NAME, CPU%, MEM,
  STATUS, UPTIME) to stdout and exits. The NAME column uses box-drawing tree
  connectors matching the TUI view. Width adapts to the terminal when stdout
  is a TTY, falls back to 120 columns when piped.
- `--json` flag: prints a versioned JSON snapshot (schema_version=1) to
  stdout and exits. `--pretty` adds indentation. The schema includes system
  stats, an agent summary, and the full recursive process tree with activity
  state. See `docs/output-schema.md` for the field reference.
  **Stability promise:** `schema_version=1` will never remove or rename
  fields; additive changes are allowed without a version bump; breaking
  changes increment `schema_version` to 2.
- `--generate-completions <shell>` flag: prints a shell-completion script
  for bash, zsh, fish, powershell, or elvish to stdout and exits. Uses
  `clap_complete`. See README for per-shell install instructions.
- 3-sample rolling CPU average in `ProcessScanner`: raw per-tick CPU
  readings are averaged over a 3-sample window per PID. The smoothed value
  flows into sparklines and idle/active classification, reducing noise from
  transient spikes.
- Time-in-state color escalation for idle root processes: idle badges now
  change color based on how long the process has been continuously idle
  (Fresh <60s, Warning 60s–5m, Stale >5m). Warning and Stale tiers append
  a duration suffix to the badge (e.g. `○(3m)`, `○(12m)`) for accessibility.
- `docs/output-schema.md`: full field reference, stability promise, and an
  example JSON snapshot.
- New dependencies: `clap` 4, `clap_complete` 4, `serde_json` 1, `time` 0.3
  (all MIT/Apache-2.0 licensed).
- Internal `TelemetryProvider` trait and `TelemetryPipeline` runner as the
  foundation for per-agent telemetry enrichment (tokens, cost, context %)
  in later releases. No user-visible behavior change.
- Continuous integration via GitHub Actions: formatting, Clippy (`-D warnings`),
  tests, doc build, `cargo-deny`, and `cargo-audit` across Ubuntu and macOS,
  on stable and MSRV.
- Release workflow that cross-builds tagged versions for
  `x86_64`/`aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`, and
  `x86_64`/`aarch64-apple-darwin`, with SHA-256 checksums.
- Supply-chain guardrails: `deny.toml` license/advisory/source policy,
  Dependabot, `SECURITY.md` disclosure policy.
- Contributor scaffolding: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  issue templates, pull request template.

### Changed
- Arg parsing migrated from hand-rolled `std::env::args` to `clap` derive
  macros. `--version` / `-V`, `--help` / `-h` behavior is preserved;
  `--json` and `--list` are mutually exclusive via a clap `ArgGroup`.
- Pinned MSRV to Rust 1.85.

## [0.7.1] - 2026-04-13

### Fixed
- Chart background now uses the active theme color instead of the terminal's
  default background, so light themes render correctly.

## [0.7.0] - 2026-04-12

### Added
- Idle / active classification for agent root processes, based on a rolling
  CPU-usage window.
- Search and filter via `/`, with parent-chain preservation so matching
  children still show their parent process.
- Subtree statistics aggregation and an agent summary shown in the status bar.
- Light themes with full-coverage background colors.

[Unreleased]: https://github.com/leboiko/claude-codex-pid-inspector/compare/v0.7.1...HEAD
[0.7.1]: https://github.com/leboiko/claude-codex-pid-inspector/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/leboiko/claude-codex-pid-inspector/releases/tag/v0.7.0
