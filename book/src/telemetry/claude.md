# Claude provider

The Claude telemetry provider reads the on-disk state a running Claude Code
session maintains and surfaces it as [`AgentTelemetry`](../output/json.md).

## Data sources

| Path | What we read | Why |
|---|---|---|
| `~/.claude/sessions/{pid}.json` | PID → session metadata (session ID, cwd, start time, version) | Direct PID correlation; no fuzzy matching |
| `~/.claude/projects/{cwd-slug}/{sessionId}.jsonl` | Transcript events | Token counts, tool calls, stop reasons |
| `/tmp/claude-{uid}/{cwd-slug}/{sessionId}/tasks/` | Subagent task files | Count of active subagents |

`{cwd-slug}` is the working directory with `/` replaced by `-`.

The transcript is **tailed incrementally**. The provider persists a byte
offset per session and reads only new bytes on each tick. File shrinkage
(truncation, rotation) is detected and the offset resets to 0.

## What we extract

From each `assistant` event's `message.usage`:

- `input_tokens`, `output_tokens` (cumulative, summed across turns)
- `cache_read_input_tokens`, `cache_write_input_tokens`
- The **latest** message's usage gives the per-turn context estimate

From each `assistant` event's `message.stop_reason`:

- `last_stop_reason` ("end_turn" / "tool_use" / ...)

From `user`-event `tool_result` blocks and `assistant`-event `tool_use`
blocks:

- Unresolved `tool_use_id`s → `pending_tool` if the last assistant message
  ended in `tool_use`

## Pricing

Hardcoded in `src/telemetry/claude/pricing.rs` with a
`CLAUDE_PRICING_AUDITED_AT` constant.

| Model prefix | Input | Output | Cache write | Cache read | Window |
|---|---|---|---|---|---|
| `claude-opus-4-7` (1M) | $15 | $75 | $18.75 | $1.50 | 1 000 000 |
| `claude-opus-4-*` | $15 | $75 | $18.75 | $1.50 | 200 000 |
| `claude-sonnet-4-*` | $3 | $15 | $3.75 | $0.30 | 200 000 |
| `claude-haiku-4-*` | $1 | $5 | $1.25 | $0.10 | 200 000 |
| fallback | Sonnet rates | — | — | — | 200 000 |

Match order is **longest-prefix-first** so the 1M-context Opus 4.7 variant
matches before the generic `claude-opus-4-*`.

**Every cost surfaces with `cost_is_estimated: true`.** We're reading from a
local table, not an authoritative bill.

## Status inference

Priority hierarchy, top match wins:

1. CPU > 5% → `processing`
2. `pending_tool` is Some → `needs_input`
3. Last assistant `stop_reason == "end_turn"` and last event > 10 min ago → `idle`
4. Last assistant `stop_reason == "end_turn"` and recent → `waiting_input`
5. Default → `unknown`

## Schema stability

Claude's transcript format is undocumented as a public API. The parser uses
`#[serde(other)]` catchalls and skips lines it doesn't understand rather
than failing the tick. If a future Claude release changes the schema, the
worst case is telemetry going to `null` until we audit and update.

The pricing table is audited before each minor release. See [Stability and
compatibility](../stability.md).
