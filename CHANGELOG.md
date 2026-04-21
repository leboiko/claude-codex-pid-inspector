# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Curated focus filters** (`F` key, cycles): five lenses selectable in the
  tree view — `all` (default, no filter), `attention` (status `NeedsInput` or
  context ≥ 80%), `high-cpu` (CPU ≥ 30%), `high-context` (context ≥ 80%),
  `recent` (started within the last 10 minutes). Pressing `F` clears the
  free-text search; starting `/` input resets the lens to `all`. A filter pill
  `FILTER: <label>  (N/M)` is now always visible in the status bar so the
  active lens is always apparent.
- **Project grouping** (`g` key, toggle): re-organises the tree view so all
  sessions sharing the same working directory appear under a group header row
  (`▼ project-slug   N sessions · $X.XX · ctx YY% avg`). Groups are sorted
  most-recently-active first; empty groups (all members filtered out) are
  hidden. `Space` or `Enter` on a header row collapses or expands the group.
  Grouping, the curated filter, and the agent view (`T`) are independent
  toggles.
- **Help overlay** (`?` key): centered modal (70% × 80% terminal) listing all
  keybindings in six categories — Navigation, View, Filter, Sort, Action,
  System. `Esc` closes; any other bound key closes the overlay and then
  executes the action.
- **Terminal-tab jump** (`Tab` in detail view): focuses the terminal pane that
  owns the selected process's TTY. Supported terminals: **tmux** (detected via
  `$TMUX`), **Kitty** (via `$KITTY_WINDOW_ID`), **iTerm2** on macOS (via
  `TERM_PROGRAM=iTerm.app`). **WezTerm** is detected but not yet implemented
  (returns a descriptive message). A 3-second status-bar flash reports success
  (`Jumped to tmux session:1.0`) or failure. The `Tab jump to terminal` footer
  hint is shown only when a supported terminal is detected. `Tab` in the tree
  view is unchanged (cycles sort column).
- **Codex CLI telemetry** (Phase 3): each detected Codex CLI root process is
  now enriched with data read incrementally from its on-disk rollout file
  (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`). Exposed in the TUI,
  detail panel, and `--json` output:
  - cumulative input/output/cache tokens and an estimate of context window
    utilisation derived from the `token_count` events in the rollout file
  - a cost-in-USD estimate from a pricing table pinned at `2026-04-21`
    covering GPT-5, o4-mini, o4, o3-mini, o3, GPT-4.1, GPT-4o, and GPT-4
    model families; all costs flagged as estimated
  - the name of the pending tool call when a `function_call` or
    `custom_tool_call` has no matching output yet
  - `kind: "codex"` in `--json` output (Claude sessions emit `kind: "claude"`)
  - `CODEX_HOME` resolution from the live process environment (macOS `ps eww`,
    Linux `/proc/{pid}/environ`) with fallback to `~/.codex`
  - PID-to-rollout-file correlation via filename timestamp (±90 s) — no
    pidfile required
  - Incremental tailing: only new bytes are read each tick; file truncation
    (session restart) resets aggregates automatically
  - Privacy: tool `arguments` and tool `output` fields are never deserialized
    or stored
- **Claude telemetry**: each detected Claude Code root process is now enriched
  with data read from its on-disk state (`~/.claude/sessions/{pid}.json` and
  `~/.claude/projects/**/*.jsonl`). Exposed in the TUI, detail panel, and
  `--json` output:
  - cumulative input/output/cache tokens and an estimate of the current
    context window utilisation
  - a cost-in-USD estimate derived from a pricing table pinned at
    `2026-04-21` (Opus 4.7, Sonnet 4.x, Haiku 4.5) — costs are flagged as
    estimated
  - the name of the pending tool call (if the session is awaiting approval),
    the last model `stop_reason`, and the count of active subagent tasks
  - an inferred `AgentStatus` (processing / needs_input / waiting_input /
    idle) combining CPU pressure with transcript signals
- **Agent view** (`T` key): swaps `CPU%`/`Memory`/`Command` for
  `Ctx%`/`Cost`/`Tokens`/`Tool` columns so the agent-specific telemetry is
  first-class in the table. Base view remains the default and is unchanged.
  Status bar and footer show when the agent view is active.
- **Pending-tool badge**: a `⚑` badge appears in the Name cell next to the
  activity badge when a Claude session is awaiting tool-call approval.
- **Extended detail panel** for Claude sessions: token stats, a 20-cell
  decay-score bar (`Decay NN`), the pending tool call's name and truncated
  input, a scrollable list of recent assistant messages, and a subagent
  summary. Non-Claude rows keep their existing detail layout.
- `telemetry` sub-object added to each process entry in the `--json` schema
  (additive; `schema_version` stays at `1`). Documented in
  `docs/output-schema.md` together with the privacy guarantee: we serialize
  only aggregate counters, never transcript content.
- Char-safe `truncate_chars` helper in `ui/format.rs` for clipping
  user-facing text without risking a panic mid-grapheme.
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

### Fixed
- Transcript preview no longer panics when the 80-character truncation
  boundary lands inside a multi-byte UTF-8 sequence (e.g. an em dash). A
  regression test in `telemetry::claude::parser` guards the fix.

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
