# Contributing

See [CONTRIBUTING.md](https://github.com/leboiko/claude-codex-pid-inspector/blob/master/CONTRIBUTING.md)
in the repository for the canonical guide — dev setup, PR checklist,
project layout, and release process.

## Fast path for newcomers

```sh
git clone https://github.com/leboiko/claude-codex-pid-inspector.git
cd claude-codex-pid-inspector
cargo test
cargo run
```

Run the same checks CI does:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-features
cargo deny check
```

## Code of conduct

This project follows the [Contributor Covenant 2.1](https://github.com/leboiko/claude-codex-pid-inspector/blob/master/CODE_OF_CONDUCT.md).

## Reporting bugs

Open an issue with:

- The output of `agentop --version`
- Your OS and terminal emulator
- Steps to reproduce
- What you expected versus what happened

For sensitive issues, follow [`SECURITY.md`](https://github.com/leboiko/claude-codex-pid-inspector/blob/master/SECURITY.md)
rather than the public tracker.
