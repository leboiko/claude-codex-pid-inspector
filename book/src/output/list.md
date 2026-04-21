# List mode

```sh
agentop --list
```

Prints a plain-text process table to stdout and exits. Matches the TUI's
default view minus the sparkline.

Example:

```
PID      NAME                                    CPU%        MEM STATUS     UPTIME
-------- --------------------------------------- -------- ---------- ---------- ----------
31133    ? ▼ claude                                 6.9%   840.8 MB Runnable   0d 1h 29m
15939    ├─ ▼ zsh                                    0.0%     3.0 MB Runnable   0m 0s
15943      ├─ head                                   0.0%     1.2 MB Runnable   0m 0s
15942      └─ agentop                                1.4%    11.0 MB Runnable   0m 0s
...
```

## Behaviour

- Width adapts to the TTY when stdout is a terminal
- Piped output (non-TTY) uses a 120-column default and emits no ANSI escapes
- Two sysinfo refreshes 500 ms apart are performed before printing, so CPU
  deltas are meaningful
- Exit status is `0` on success; non-zero only on fatal error

## When to use

- Grep-able snapshots of running agent sessions
- Terminal-less invocations (CI, remote `ssh` with no controlling tty)
- Quick pipelines: `agentop --list | awk '$3 > 10.0 {print}'`

For richer data (cost, tokens, context %), use `--json` with
[`jq`](https://stedolan.github.io/jq/).
