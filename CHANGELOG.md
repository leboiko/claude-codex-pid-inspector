# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
