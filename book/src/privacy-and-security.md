# Privacy and security

agentop is designed around a hard read-only boundary. This chapter explains
exactly what the tool reads, what it writes, and what it deliberately does
not do.

## What we read

| Source | Used for |
|---|---|
| `sysinfo` / `ps` / `/proc` | CPU, memory, command line, working directory, PID tree |
| `~/.claude/sessions/*.json` | PID → Claude session correlation |
| `~/.claude/projects/**/*.jsonl` | Claude transcripts, for tokens / cost / pending tool |
| `/tmp/claude-{uid}/**/tasks/` | Claude subagent count |
| `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | Codex rollout events, for tokens / cost / pending tool |
| `/proc/{pid}/environ` (Linux) / `ps eww` (macOS) | `CODEX_HOME` override per PID |

All reads are on files your user already owns and can read. No elevated
privileges, no setuid bits, no platform-specific escalation.

## What we write

| Path | Contents |
|---|---|
| `$XDG_CONFIG_HOME/agentop/config.toml` | Persisted theme + graph-style preferences |
| stdout / stderr | When invoked with `--list`, `--json`, `--generate-completions`, or any diagnostic flag |

That's the complete list. We do not:

- Install or modify Claude Code / Codex CLI hooks, commands, skills, plugins,
  or configuration
- Write to `~/.claude/`, `~/.codex/`, or any path outside `~/.config/agentop/`
- Spawn subprocesses except when the user explicitly triggers `x` (kill),
  the `Tab` terminal-jump (shells out to `tmux` / `osascript` / `kitten`),
  or the one-shot macOS `ps eww` read for environment parsing

## No network

agentop makes zero outbound network requests. It has no telemetry, no
update checker, no error reporter, and no license server. `cargo-deny` is
configured to deny `openssl-sys`, `openssl`, and `native-tls` in the
dependency graph to close the door on accidental HTTP via a transitive dep.

## JSON output boundary

`--json` emits only **aggregate counters** and **metadata**. It never
serialises transcript content, tool arguments, tool outputs, assistant
messages, or anything else that could contain user-authored or
model-authored text.

Specifically:

- Claude `message.content[]` blocks are **never** serialised
- Codex `response_item.payload` content is **never** serialised
- Tool `input` / `arguments` JSON is **never** serialised
- Tool `output` is **never** serialised

What IS serialised: token counts, cost estimates, context fractions, pending
tool **names**, last stop reason strings, subagent counts, session IDs,
model names, working directories, and PIDs.

## When to worry

If you're shipping `agentop --json` output across a trust boundary (into a
log aggregator, a chat channel, a ticket system), verify it against your
organisation's policy. Working-directory paths and PIDs may themselves be
sensitive in some environments. See [JSON schema](output/json.md) for the
full field list.

## Disclosure

Report a suspected vulnerability via the process in [SECURITY.md](https://github.com/leboiko/claude-codex-pid-inspector/blob/master/SECURITY.md)
rather than in a public issue.
