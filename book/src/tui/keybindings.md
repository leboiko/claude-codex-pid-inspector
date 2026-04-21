# Keybindings

Every binding is shown in the in-app help overlay — press `?` to see the
live list.

## Tree view (default)

| Key | Action |
|---|---|
| `q`, `Ctrl+C` | Quit |
| `↑`, `k` | Move selection up |
| `↓`, `j` | Move selection down |
| `Enter` | Open detail view for selected row |
| `Space` | Expand / collapse selected node (or group when grouping is on) |
| `Tab` | Cycle sort column forward |
| `Shift+Tab` | Cycle sort column backward |
| `s` | Toggle sort direction |
| `x` | Kill selected process (with confirmation) |
| `c` | Open config popup |
| `/` | Enter free-text filter |
| `F` | Cycle curated focus filter |
| `g` | Toggle project grouping |
| `T` | Toggle agent view (Ctx% / Cost / Tokens / Tool columns) |
| `?` | Show help overlay |
| `z` | Clear any active filter |

## Detail view

| Key | Action |
|---|---|
| `Esc` | Return to tree view |
| `q`, `Ctrl+C` | Quit |
| `x` | Kill selected process (with confirmation) |
| `c` | Open config popup |
| `Tab` | Jump to the terminal hosting this PID (tmux, iTerm2, Kitty) |
| `?` | Show help overlay |

## While the help overlay is open

| Key | Action |
|---|---|
| `Esc` | Close the overlay |
| Any other bound key | Close, then execute that key's action |

## While the free-text filter bar is active

| Key | Action |
|---|---|
| `Esc` | Clear filter and dismiss the bar |
| `Backspace` | Delete one character |
| `Enter` | Open detail view for highlighted row |
| `↑↓`, `j`/`k` | Move selection (search results scroll under the bar) |
| Other printable chars | Appended to the query |
