# Quickstart

## Launching

```sh
agentop
```

The TUI opens in an alternate screen buffer, so your terminal scrollback is
not disturbed. Quit with `q` or `Ctrl+C`.

If no Claude or Codex sessions are running, you'll see only the base process
tree — which is fine; the agent view will fill in as soon as you start one.

## The first five minutes

### See your agent sessions

Press **`T`** to switch to the agent view. The columns change to:

```
PID | Name | Status | Ctx% | Cost | Tokens | Tool | Uptime
```

Each Claude or Codex root row now shows its model, current context-window
utilisation, cumulative cost in USD, and the name of any pending tool call.

### Focus on what needs attention

Press **`F`** to cycle through curated filters:

| Filter | What it shows |
|---|---|
| `all` | every process (default) |
| `attention` | sessions with a pending tool call or ≥ 80% context |
| `high-cpu` | smoothed CPU ≥ 30% |
| `high-context` | context ≥ 80% |
| `recent` | started in the last 10 minutes |

The status bar shows which filter is active: `FILTER: high-cpu  (4/11)`.

### Group by project

Press **`g`** to cluster rows by working directory. Each group gets a header
row with the slug, session count, total cost, and average context %:

```
▼ /Users/me/project    3 sessions · $4.21 · ctx 47% avg
```

Press **Space** on a header to collapse / expand the group.

### Help is one key away

Press **`?`** to open a modal listing every keybinding. `Esc` dismisses.

### Drill into a session

Press **Enter** on a row to see the detail panel: token stats, a decay-score
bar for Claude sessions, the pending tool call (if any), the last few
assistant messages, and a subagent summary.

From the detail view, press **Tab** to jump focus to the terminal hosting
that PID — supported on tmux, iTerm2, and Kitty. See the
[Terminal support matrix](terminals.md) for the current state.

### Inspect non-interactively

If you just want the data:

```sh
agentop --list     # plain-text table
agentop --json     # single-line JSON snapshot
agentop --json --pretty | jq '.processes[] | select(.kind == "claude")'
```

See [JSON schema](output/json.md) for the full field reference.

## What's safe to try

Everything in this tool is read-only except `x` (kill), which prompts for
confirmation. The agent view, grouping, help overlay, and all filters are
purely visual. You cannot accidentally modify your Claude or Codex state.
