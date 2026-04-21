# agentop JSON output schema

`agentop --json` emits a single-line JSON object (schema version 1).
`agentop --json --pretty` emits the same object with 4-space indentation.

---

## Stability promise

Schema version 1 will **never** remove or rename a field. Additive changes
(new fields, new optional members in nested objects) are allowed without a
version bump and will be noted in `CHANGELOG.md` as a minor change.
A schema-breaking change (field removal, rename, type change) increments
`schema_version` to 2.

Consumers **must** check `schema_version` before parsing. A `schema_version`
of 1 guarantees the fields listed below are always present (never absent
due to `skip_serializing_if`).

---

## Top-level fields

| Field             | Type    | Description |
|-------------------|---------|-------------|
| `schema_version`  | integer | Always `1` for this schema |
| `generated_at`    | string  | RFC 3339 UTC timestamp (`2026-04-21T13:45:00Z`) |
| `agentop_version` | string  | SemVer string from `Cargo.toml` |
| `system`          | object  | System-wide resource snapshot (see below) |
| `agent_summary`   | object  | Aggregate counts and usage (see below) |
| `processes`       | array   | Root-level agent processes (see below) |

---

## `system` object

| Field                | Type    | Description |
|----------------------|---------|-------------|
| `cpu_usage_percent`  | float   | Global CPU percent across all cores |
| `total_memory_bytes` | integer | Total installed physical memory |
| `used_memory_bytes`  | integer | Currently resident memory |
| `total_swap_bytes`   | integer | Total swap space |
| `used_swap_bytes`    | integer | Currently used swap |
| `cpu_count`          | integer | Number of logical CPUs |

---

## `agent_summary` object

| Field                | Type    | Description |
|----------------------|---------|-------------|
| `claude_count`       | integer | Number of Claude Code root processes |
| `codex_count`        | integer | Number of Codex CLI root processes |
| `total_cpu_percent`  | float   | Total CPU % across all agent subtrees |
| `total_memory_bytes` | integer | Total memory (bytes) across all agent subtrees |

---

## `processes` array (recursive)

Each entry in `processes` is a root agent process. Its `children` field
contains the same structure recursively. Non-agent child processes appear
inside `children` with `kind: null`.

| Field                    | Type            | Description |
|--------------------------|-----------------|-------------|
| `pid`                    | integer         | Process ID |
| `parent_pid`             | integer or null | Parent PID; `null` for root entries |
| `kind`                   | string or null  | `"claude"`, `"codex"`, or `null` |
| `name`                   | string          | Short OS process name |
| `display_name`           | string          | Friendly name (e.g. `"claude"` even when OS name is `"node"`) |
| `cmd`                    | string[]        | Full argv split into tokens |
| `exe_path`               | string or null  | Absolute path to executable |
| `cwd`                    | string or null  | Working directory |
| `cpu_percent`            | float           | CPU % (3-sample rolling average) |
| `memory_bytes`           | integer         | Resident memory in bytes |
| `status`                 | string          | OS status string (`"Run"`, `"Sleep"`, etc.) |
| `start_time_unix`        | integer         | Unix epoch timestamp when the process started |
| `uptime_seconds`         | integer         | Seconds the process has been running |
| `activity_state`         | string or null  | `"active"`, `"idle"`, `"unknown"`, or `null` (non-roots) |
| `activity_state_seconds` | integer or null | Seconds in the current activity state; `null` for non-roots |
| `telemetry`              | object or null  | Provider-specific session telemetry (see below); `null` when no provider is active |
| `children`               | array           | Same shape, recursive |

### `activity_state` values

| Value     | Meaning |
|-----------|---------|
| `"active"` | CPU exceeded idle threshold in at least one recent sample |
| `"idle"`   | All recent samples are below the idle threshold |
| `"unknown"` | Not enough samples collected yet |
| `null`    | This entry is a non-root child process |

---

## `telemetry` object (Claude provider)

Present on root-level Claude process entries when `~/.claude/sessions/` exists
and a matching session file has been found for the PID. `null` otherwise.

`schema_version` remains at **1** — this field is additive.

| Field                  | Type            | Description |
|------------------------|-----------------|-------------|
| `provider`             | string          | Always `"claude"` for the Claude provider |
| `session_id`           | string or null  | Claude session UUID |
| `model`                | string or null  | Model identifier from the most recent turn |
| `status`               | string or null  | Agent status: `"processing"`, `"needs_input"`, `"waiting_input"`, `"idle"`, or `"unknown"` |
| `input_tokens`         | integer         | Cumulative input tokens for this session |
| `output_tokens`        | integer         | Cumulative output tokens for this session |
| `cache_write_tokens`   | integer         | Cumulative cache-write tokens |
| `cache_read_tokens`    | integer         | Cumulative cache-read tokens |
| `context_tokens`       | integer or null | Context token estimate from the most recent turn |
| `context_window`       | integer or null | Model context window size (from pricing table) |
| `context_fill_percent` | float or null   | `context_tokens / context_window * 100` |
| `cost_usd`             | float           | Accumulated session cost in USD (estimated) |
| `cost_is_estimated`    | boolean         | Always `true` — cost is derived from a local pricing table |
| `pending_tool`         | string or null  | Name of the tool currently awaiting a result, if any |
| `decay_score`          | integer or null | Context-health score 0–100 (higher = more pressure); Phase 2 uses context fill only |
| `subagent_count`       | integer         | Number of subagent task files found under `/tmp/claude-{uid}/...` |
| `last_event_timestamp` | string or null  | RFC 3339 timestamp of the most recent assistant event |

### `status` values

| Value            | Meaning |
|------------------|---------|
| `"processing"`   | CPU usage above 5% — model is actively generating |
| `"needs_input"`  | Last turn stopped with `tool_use` and a tool result is pending |
| `"waiting_input"` | Last turn stopped with `end_turn`; a recent event was seen |
| `"idle"`         | Last turn stopped with `end_turn`; no recent events |
| `"unknown"`      | Not enough information to classify |

---

## Privacy

The `telemetry` object intentionally omits all transcript content.
The `--json` output **never** includes message text, tool arguments, or any
other content from the conversation transcript, regardless of what the TUI
detail view displays.

This promise is enforced at the serialization boundary in `src/output/json.rs`:
`TelemetryJson` (the serialized form) is a separate type from `AgentTelemetry`
(the in-memory form). The conversion function `build_telemetry_json()` only
copies aggregated counters, never the `recent_messages` field.

---

## Example (pretty-printed)

```json
{
  "schema_version": 1,
  "generated_at": "2026-04-21T13:45:00Z",
  "agentop_version": "0.7.1",
  "system": {
    "cpu_usage_percent": 42.1,
    "total_memory_bytes": 17179869184,
    "used_memory_bytes": 8421376,
    "total_swap_bytes": 4294967296,
    "used_swap_bytes": 0,
    "cpu_count": 10
  },
  "agent_summary": {
    "claude_count": 2,
    "codex_count": 1,
    "total_cpu_percent": 15.2,
    "total_memory_bytes": 2147483648
  },
  "processes": [
    {
      "pid": 1234,
      "parent_pid": null,
      "kind": "claude",
      "name": "claude",
      "display_name": "claude",
      "cmd": ["claude", "--resume", "abc"],
      "exe_path": "/usr/local/bin/claude",
      "cwd": "/Users/me/project",
      "cpu_percent": 3.4,
      "memory_bytes": 1073741824,
      "status": "Run",
      "start_time_unix": 1714000000,
      "uptime_seconds": 300,
      "activity_state": "active",
      "activity_state_seconds": 42,
      "telemetry": {
        "provider": "claude",
        "session_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "model": "claude-opus-4-7",
        "status": "idle",
        "input_tokens": 42000,
        "output_tokens": 3100,
        "cache_write_tokens": 12000,
        "cache_read_tokens": 28000,
        "context_tokens": 82000,
        "context_window": 1000000,
        "context_fill_percent": 8.2,
        "cost_usd": 0.87,
        "cost_is_estimated": true,
        "pending_tool": null,
        "decay_score": 3,
        "subagent_count": 2,
        "last_event_timestamp": "2026-04-21T13:44:55Z"
      },
      "children": [
        {
          "pid": 1235,
          "parent_pid": 1234,
          "kind": null,
          "name": "node",
          "display_name": "node",
          "cmd": ["node", "/path/to/script.js"],
          "exe_path": "/usr/local/bin/node",
          "cwd": "/Users/me/project",
          "cpu_percent": 1.2,
          "memory_bytes": 536870912,
          "status": "Run",
          "start_time_unix": 1714000010,
          "uptime_seconds": 290,
          "activity_state": null,
          "activity_state_seconds": null,
          "telemetry": null,
          "children": []
        }
      ]
    }
  ]
}
```
