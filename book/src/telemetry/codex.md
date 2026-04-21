# Codex provider

The Codex telemetry provider reads `~/.codex/sessions/` rollout files and
surfaces them via [`AgentTelemetry`](../output/json.md). The rollout format
is actually **simpler** than Claude's transcript: every event is a tagged
union with a known `type`, and the model's context-window size is written
directly into each `token_count` event — no per-model lookup table needed
for the context fraction.

## Data sources

| Path | What we read |
|---|---|
| `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | Session meta, turn context, token counts, tool calls |
| `/proc/{pid}/environ` (Linux) | `CODEX_HOME` override |
| `ps eww -p <pid>` (macOS) | Same override via appended env block |

`CODEX_HOME` lets users point Codex at a non-default state directory; the
provider reads it once per PID and caches the result.

## PID correlation

**Filenames are unreliable for correlation.** Codex writes timestamps in
the host's *local* time, not UTC. On a UTC-3 machine a session that started
at `13:43:38Z` produces a filename with `10-43-38` — naive UTC parsing
would match nothing.

The provider correlates by opening each candidate rollout file and reading
the authoritative UTC timestamp from its first `session_meta` event's
`payload.timestamp`. Matches within ±90 s of the process's start time.

## What we extract

From each `event_msg.payload` where `type == "token_count"` (the latest
populated `info` wins):

- `total_token_usage.{input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens}` — session cumulative
- `last_token_usage.input_tokens` — **the live context signal** (cumulative `total_tokens` grows unbounded across turns and would report >100% on any long session)
- `model_context_window`

From each `response_item.payload`:

- `function_call` / `custom_tool_call` events increment the unresolved
  call-id set
- `function_call_output` / `custom_tool_call_output` clear it
- If the last response item is an unresolved call, `pending_tool` gets its
  name

From each `turn_context.payload`:

- `model` (e.g. `gpt-5.4`, `gpt-5-codex`)

## Pricing

Hardcoded in `src/telemetry/codex/pricing.rs` with a
`CODEX_PRICING_AUDITED_AT` constant. Rates are **estimates** aligned with
public OpenAI pricing tiers; every cost surfaces with
`cost_is_estimated: true`.

The table covers `gpt-5*`, `gpt-5-codex`, and `o3/o4` reasoning families.
Reasoning output tokens are billed at the output rate.

## Fields Codex leaves null

| Field | Reason |
|---|---|
| `cache_write_tokens` | Codex rollout exposes cached-input but not cache-write separately |
| `decay_score` | The decay formula relies on error-rate and file-edit signals Codex doesn't expose |
| `subagent_count` | Codex has no subagent task files |
| `last_stop_reason` | Codex doesn't emit a Claude-style per-turn stop reason |

## Schema stability

The rollout format is internal to Codex. The parser ignores unknown event
types via a `#[serde(other)]` catchall. If a future Codex release changes
the schema, telemetry falls back to `null` until we audit. See
[Stability and compatibility](../stability.md) for the audit cadence.
