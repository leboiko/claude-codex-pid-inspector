# claude-codex-pid-inspector

A terminal UI (TUI) process inspector that watches your system for running
Claude Code and OpenAI Codex CLI processes, displays them as an expandable
process tree, and lets you drill into live CPU and memory metrics for any
individual process.

---

## Screenshot

```
+-----------------------------------------------------------------------+
|                        Process Inspector                              |
+--------+----------------------+------+----------+-------+------+------+
| PID    | Name                 | CPU% | Memory   | Status| Cmd  |Uptime|
+--------+----------------------+------+----------+-------+------+------+
| 12345  | ▼ claude             | 2.3% | 143.2 MB | Sleep | ...  | 4m 8s|
| 12346  |   ├─ node            | 0.1% |  48.0 MB | Sleep | ...  | 4m 8s|
| 12347  |   └─ node            | 0.0% |  32.4 MB | Sleep | ...  | 4m 7s|
| 67890  | ▶ codex              | 1.1% |  89.5 MB | Run   | ...  | 1m 2s|
+--------+----------------------+------+----------+-------+------+------+
q: Quit  ↑/↓: Navigate  Enter: Details  Space: Expand/Collapse  r: Refresh
```

The **tree view** lists every detected Claude / Codex root process in
orange (Claude) or green (Codex), with their child processes indented in
grey. Pressing `Enter` opens a **detail view** for the selected process,
showing a full info table alongside live sparkline charts for CPU usage
and memory over the last 30 samples.

---

## Features

- Automatic detection of Claude Code and OpenAI Codex CLI processes using
  multiple heuristics (name, argv, and exe path).
- Parent/child process tree built from OS parent-PID relationships, with
  expand and collapse support.
- Live refresh every 2 seconds on a background blocking thread, so the
  async UI reactor is never stalled.
- Per-process rolling history of the last 30 CPU and memory samples,
  visualised as sparkline charts in the detail view.
- Expansion state is preserved across refresh cycles — collapsing a
  subtree is not reset when new data arrives.
- Context-sensitive one-line footer showing only the key bindings relevant
  to the active view.
- Safe terminal restoration on both clean exit and panic.

---

## Installation

Requires [Rust](https://rustup.rs/) 1.70 or later.

```sh
git clone <repo-url>
cd pid-inspector
cargo build --release
# The binary is at ./target/release/pid-inspector
```

To install it to your Cargo bin directory:

```sh
cargo install --path .
```

---

## Usage

```sh
pid-inspector
```

The application opens in an alternate screen buffer (your terminal history
is not disturbed) and begins scanning immediately.

### Keybindings

#### Tree view

| Key          | Action                              |
|--------------|-------------------------------------|
| `q`          | Quit                                |
| `Ctrl+C`     | Quit (universal)                    |
| `Up` / `k`   | Move selection up                   |
| `Down` / `j` | Move selection down                 |
| `Enter`      | Open detail view for selected row   |
| `Space`      | Expand / collapse selected node     |

#### Detail view

| Key    | Action              |
|--------|---------------------|
| `Esc`  | Return to tree view |
| `q`    | Quit                |
| `Ctrl+C` | Quit (universal)  |

---

## Process Detection

Detection logic is implemented in `src/process/filter.rs` and operates on
a snapshot of every process's name, argv, and exe path.

### Claude Code

A process is classified as Claude if **any** of the following conditions
hold:

1. `process.name == "claude"`
2. `argv[0] == "claude"` — catches cases where `sysinfo` reports the
   underlying engine name (e.g. `"node"`) instead of the display title.
3. The exe path contains `.local/share/claude` — covers the typical
   installation directory on Linux/macOS.
4. The process name consists entirely of digits and dots (a version string
   such as `"1.2.3"`) **and** the exe path contains `"claude/versions"` —
   matches versioned Electron bundles shipped by the installer.

### OpenAI Codex CLI

A process is classified as Codex if **any** of the following conditions
hold:

1. `process.name == "codex"`
2. `argv[0] == "codex"`
3. Any argv token contains `"@openai/codex"` or `"codex.js"` — catches
   `npx`-launched invocations where the process name is `"node"`.

All child processes (those whose parent PID matches a detected root) are
included in the tree regardless of their own name.

---

## Architecture

```
src/
  main.rs          Entry point; tokio runtime setup, event loop, scanner
                   channel wiring, and frame rendering dispatch.
  app.rs           Central application state (App) and all state mutations.
                   Translates Actions into state changes, manages history
                   ring buffers, and drives flat-list rebuilds.
  action.rs        Enum of all discrete user actions (Quit, MoveUp, ...).
  event.rs         Async EventHandler that multiplexes crossterm key events,
                   periodic Tick signals, and Render signals over a single
                   channel.
  tui.rs           Terminal initialisation / restoration helpers and the
                   panic hook that ensures raw mode is always cleaned up.
  process/
    info.rs        ProcessInfo struct: owned snapshot of one OS process.
    filter.rs      is_claude_process / is_codex_process predicates.
    scanner.rs     ProcessScanner wrapping sysinfo::System; incremental
                   refresh with CPU delta seeding.
    tree.rs        Forest construction from flat snapshots, flatten_visible,
                   and expand/collapse state helpers.
    mod.rs         Public re-exports.
  ui/
    tree_view.rs   Renders the scrollable Table with box-drawing tree
                   connectors, colour-coded rows, and a scrollbar.
    detail_view.rs Renders the detail panel: header, info table, CPU
                   sparkline, memory sparkline, and full command line.
    footer.rs      Context-sensitive one-line key-binding hint bar.
    styles.rs      Shared Style / Color constants.
    mod.rs         Public re-exports.
```

The event loop runs at two independent rates: a **tick rate** of 2 seconds
triggers a background scan, and a **render rate** of ~30 fps drives
redraws. The scanner runs on a dedicated `tokio::task::spawn_blocking`
thread to avoid blocking the async reactor during `sysinfo` syscalls.

---

## Tech Stack

| Crate         | Version | Purpose                                      |
|---------------|---------|----------------------------------------------|
| `ratatui`     | 0.30    | TUI rendering framework                      |
| `crossterm`   | 0.29    | Cross-platform terminal I/O and event stream |
| `tokio`       | 1       | Async runtime, channels, spawn_blocking      |
| `sysinfo`     | 0.38    | Cross-platform process and system metrics    |
| `color-eyre`  | 0.6     | Error reporting and panic hook integration   |
| `strum`       | 0.26    | Enum utilities                               |
| `futures`     | 0.3     | Stream combinators for the event loop        |

---

## License

MIT
