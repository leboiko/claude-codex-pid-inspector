# Agent view

Press **`T`** to toggle. The column set changes from the process-manager
default to one optimised for Claude and Codex sessions.

## Default layout

```
PID | Name | CPU% | Memory | Status | Command | Uptime
```

The neutral process-inspector view. Unchanged from traditional `top`-style
tools; useful for spotting runaway child processes.

## Agent layout

```
PID | Name | Status | Ctx% | Cost | Tokens | Tool | Uptime
```

Codex and non-agent rows render `–` in the telemetry columns so the row
still reads cleanly.

## Cell semantics

### `Status`

One of: `processing`, `needs_input`, `waiting_input`, `idle`, `finished`,
`unknown`. Inferred from a priority hierarchy (CPU > pending-tool > last
stop reason > timestamp age) so a busy session never reads as Idle just
because its transcript lags.

### `Ctx%`

Live context-window utilisation for the most recent turn.

| Value | Colour |
|---|---|
| < 50% | green |
| 50 – 79% | yellow |
| ≥ 80% | red |

Absent sessions (e.g. very fresh) render as dim `–`.

**Claude**: sum of `input_tokens + cache_creation_input_tokens +
cache_read_input_tokens` from the most recent assistant message, divided by
the model's context window (looked up from the pricing table).

**Codex**: `last_token_usage.input_tokens / model_context_window`, both read
verbatim from the rollout file.

### `Cost`

Session-cumulative USD estimate.

| Value | Colour |
|---|---|
| < $1 | green |
| $1 – $5 | yellow |
| > $5 | red |

Both providers compute cost locally from a model pricing table, flagged
`cost_is_estimated: true` in the JSON output. See the
[Claude](../telemetry/claude.md) and [Codex](../telemetry/codex.md) chapters
for audit dates.

### `Tokens`

Combined `{in_k}k↑ {out_k}k↓`, cumulative across the session. No colour
thresholds — interpretation depends on pricing tier.

### `Tool`

Truncated name of a pending tool call (at most 10 chars, ellipsis if
longer). Bold yellow when pending; empty otherwise.

## Interaction with other toggles

`T` is orthogonal to `F` (focus filters), `g` (project grouping), and `/`
(free-text filter). Any combination is valid. The default state (all
toggles off) is the classic process-inspector view.

## Why a toggle, not always-on

Claude and Codex telemetry columns are only meaningful for agent rows.
Showing them as permanent columns would either waste screen width on `–`
cells for every shell and subprocess, or crowd out the process-manager
fundamentals. Toggling keeps both modes sharp.
