# Terminal support matrix

Pressing `Tab` in the detail view jumps focus to the terminal or pane
hosting the selected process's TTY. Here's the current state of
integration.

| Terminal | Detected via | Jump action | Status |
|---|---|---|---|
| tmux | `$TMUX` environment variable | `tmux select-pane -t <tty>` | ✅ supported |
| iTerm2 (macOS) | `TERM_PROGRAM == "iTerm.app"` | AppleScript via `osascript` | ✅ supported |
| Kitty | `$KITTY_WINDOW_ID` set | `kitten @ focus-window --match tty:<tty>` | ✅ supported |
| WezTerm | `$WEZTERM_EXECUTABLE` set | `wezterm cli activate-pane --pane-id <id>` | ⚠ detected; jump stubbed |
| Anything else | — | — | ❌ not supported; `Tab` shows a status-bar message |

## Why WezTerm is stubbed

WezTerm's `wezterm cli list` exposes pane metadata but not a direct
TTY → pane-id mapping. A reliable lookup requires walking the process
table inside the pane, which is orthogonal work we haven't tackled yet.
Detection is in place so the feature works the moment we finish the
mapping.

## Failure UX

- **Unknown terminal**: 3-second status-bar flash — `Cannot jump: terminal
  not detected (iTerm2/tmux/kitty/wezterm supported)`.
- **Self-jump** (agentop is already running in the target pane): brief
  flash — `This session is already in focus`. No-op.
- **Command failed** (e.g. `tmux` not on `$PATH`): flash — `Failed to jump: <reason>`.
- **Success**: flash — `Jumped to <terminal> <pane-id>`.

## Adding a new terminal

New adapters live under `src/terminals/`. Each implements the
`TerminalAdapter` trait with three methods:

- `name(&self)` — the display name shown in status messages
- `current_target(&self)` — returns `Some(TerminalTarget)` if the adapter is
  running us now
- `focus_by_tty(&self, tty: &str)` — shells out to the terminal's CLI

Contributions are welcome. See [Contributing](contributing.md).
