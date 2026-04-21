# Activity badges

Every Claude and Codex root row shows a single badge character at the left of
the Name cell, coloured and decorated to signal the session's live state.
Row colour (Claude orange, Codex green) stays dedicated to brand
identification; the badge is rendered in its own span and carries the
activity signal independently.

## Base shapes

| Glyph | Meaning |
|---|---|
| `●` | **Active** — CPU above the idle threshold in recent samples |
| `○` | **Idle** — below the idle threshold for the sampling window |
| _(none)_ | **Unknown** — not enough CPU samples collected yet |

## Idle escalation

The longer a session has been continuously idle, the more urgent the badge:

| Duration in Idle | Tier | Colour | Text suffix |
|---|---|---|---|
| 0 – 60 s | Fresh | gray | _(none)_ |
| 60 s – 5 min | Warning | yellow | `(3m)` |
| > 5 min | Stale | red, bold | `(12m)`, `(1h+)` |

The duration suffix makes the state legible without relying on colour —
important for users with deuteranopia or protanopia, where yellow / red /
green collapse into a similar band.

The timer **resets** every time a session re-enters Idle, so an
`Idle → Active → Idle` transition doesn't accumulate toward the Stale
threshold. A Stale badge means a genuinely uninterrupted idle period.

## Pending-tool flag

When a Claude or Codex session is awaiting user approval on a tool call, a
`⚑` flag appears immediately after the activity badge, in bold yellow:

```
● ⚑ claude
```

The Tool column (in agent view) shows the tool name.

## Why this exists

The real cost of AI coding sessions is not CPU — it's *abandoned* sessions
that accumulate idle cost or block on a tool call nobody notices. The
badges are tuned to pull attention when, and only when, something actionable
is waiting.
