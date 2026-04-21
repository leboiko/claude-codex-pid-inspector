# Security Policy

## Supported Versions

Only the latest release published to [crates.io](https://crates.io/crates/agentop) is supported with security fixes. Please update before reporting.

| Version | Supported |
|---------|-----------|
| Latest  | Yes       |
| Older   | No        |

## Scope

agentop is a read-only process inspector. Its access surface is intentionally narrow:

- Reads process metadata via `sysinfo` (equivalent to reading `/proc` on Linux or calling `ps` on macOS).
- Reads configuration files under `~/.claude/` and `~/.codex/` to display agent context.
- No network connections of any kind — agentop is entirely offline.
- No elevated privileges required or requested (no `setuid`, no `sudo`).
- No writes to disk beyond what the user explicitly configures.

Findings outside this scope (e.g., vulnerabilities in the terminal emulator, the OS, or in Claude Code / Codex CLI themselves) are out of scope for this project.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Please report security issues by emailing:

**luiseduardo.boiko@gmail.com**

Include:
- A description of the vulnerability and its potential impact.
- Steps to reproduce or a proof-of-concept (if available).
- Any suggested mitigations.

You can encrypt your message using the GPG key on [keys.openpgp.org](https://keys.openpgp.org) if you prefer.

### Response Target

I aim to acknowledge all reports within **7 days** and to provide an initial assessment within **14 days**. For confirmed vulnerabilities I will work toward a fix and coordinated disclosure. I will credit reporters in the release notes unless anonymity is requested.

## Disclosure Policy

This project follows a **coordinated disclosure** approach. Please allow reasonable time to investigate and patch before public disclosure. I will not pursue legal action against researchers acting in good faith.
