# Contributing to agentop

Thanks for your interest in contributing. agentop is a small, focused project
and contributions are very welcome — bug reports, feature requests, and pull
requests alike.

## Ground rules

- **Be kind.** This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
- **File an issue before writing a large PR.** For non-trivial changes, open
  an issue or a discussion first so we can agree on the shape of the change
  before you invest time.
- **Keep the tool focused.** agentop is a read-only process inspector. It
  deliberately does not modify Claude Code / Codex CLI configuration, install
  hooks, or kill processes outside the explicit `x` action. Proposals that
  change this scope will get pushback — not because the ideas are bad, but
  because they belong in a different tool.

## Development setup

You need Rust stable (a `rust-toolchain.toml` pins the channel). MSRV is 1.85.

```sh
git clone https://github.com/leboiko/claude-codex-pid-inspector.git
cd claude-codex-pid-inspector
cargo test
cargo run
```

Useful commands:

```sh
cargo fmt --all                                   # format
cargo clippy --all-targets -- -D warnings         # lint
cargo test --all-features                         # run tests
cargo doc --no-deps --open                        # view rustdoc
cargo deny check                                  # license/advisory check
```

## Pull request checklist

Before you open a PR, please confirm:

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --all-features` passes
- [ ] New public items carry rustdoc
- [ ] `CHANGELOG.md` has an entry under `## [Unreleased]` describing the change
- [ ] The PR description explains the motivation and includes a test plan

CI runs the same checks on Ubuntu and macOS, on stable and MSRV.

## Commit messages

We use short, imperative commit subjects (under ~70 characters) with a body
that explains the *why* rather than the *what*. Reference issues with
`Fixes #123` or `Refs #123` where applicable.

## Project layout

```
src/
  main.rs            Entry point, event loop, scanner wiring, render dispatch.
  app.rs             Central state machine; translates Actions into state changes.
  action.rs          Enum of discrete user actions.
  event.rs           Async EventHandler (crossterm + tick + render).
  tui.rs             Terminal init / restore + panic hook.
  config.rs          Persisted settings (theme, graph style).
  process/           OS-level process snapshot, filter, scanner, tree.
  telemetry/         Out-of-band per-agent telemetry (tokens, cost, ...).
  ui/                Ratatui widgets: tree view, detail view, status bar, footer.
tests/               Integration tests.
```

## Release process (maintainers)

1. Update `CHANGELOG.md` — move `[Unreleased]` entries under a new
   `## [x.y.z] - YYYY-MM-DD` header; update the link references at the bottom.
2. Bump `version` in `Cargo.toml`.
3. `cargo publish --dry-run` to sanity-check.
4. Commit, tag (`git tag -s vx.y.z -m 'Release x.y.z'`), push the tag.
5. The release workflow builds binaries, creates a GitHub Release, and
   publishes to crates.io.

## Getting help

Open an issue, start a discussion, or email the maintainer listed in
[`SECURITY.md`](SECURITY.md) for sensitive matters.
