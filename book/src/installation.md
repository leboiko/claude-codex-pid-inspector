# Installation

Requires macOS or Linux. Windows is not supported.

## From crates.io (recommended)

Needs [Rust](https://rustup.rs/) 1.85 or later.

```sh
cargo install agentop
```

`cargo install` builds release-optimised binaries by default.

## From Homebrew

> Once the tap repo is published at `leboiko/homebrew-tap`, install with:

```sh
brew install leboiko/tap/agentop
```

## From Nix

```sh
nix run github:leboiko/claude-codex-pid-inspector
```

Or add to a flake:

```nix
{
  inputs.agentop.url = "github:leboiko/claude-codex-pid-inspector";
  # ...
}
```

## From AUR (Arch Linux)

Prebuilt binary package:

```sh
yay -S agentop-bin
```

Or with any AUR helper that accepts manual submissions.

## Prebuilt binaries from GitHub Releases

Each tagged release ships SHA256-checksummed tarballs for:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

See the [Releases page](https://github.com/leboiko/claude-codex-pid-inspector/releases).
Verify the download against `SHA256SUMS` before running.

## From source

```sh
git clone https://github.com/leboiko/claude-codex-pid-inspector.git
cd claude-codex-pid-inspector
cargo install --path .
```

## Verifying the install

```sh
agentop --version
```

If `~/.cargo/bin` is not on your `PATH`, add it to your shell rc:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```
