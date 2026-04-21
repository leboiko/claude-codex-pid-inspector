# Introduction

**agentop** is a terminal UI (TUI) process inspector for developers who run
[Claude Code](https://www.anthropic.com/claude-code) or the
[OpenAI Codex CLI](https://github.com/openai/codex) alongside their regular
shell workflow.

Think of it as `top`, but aware of the two coding agents: it detects their
processes, reads their on-disk state, and surfaces the signals that actually
matter — context-window utilisation, cost, pending tool calls, and status —
in a single live view.

## What it shows

- A tree of every detected Claude and Codex session, plus their descendants
- Rolling CPU / memory / uptime — smoothed, time-in-state coloured
- Cumulative token counts and an estimated cost in USD for each agent session
- Current context-window utilisation, reading Claude and Codex transcripts
- Pending tool calls flagged with a `⚑` badge
- Project-level grouping and curated attention filters

Everything runs on-device. No network calls, no telemetry, no modifications
to Claude Code or Codex settings.

## Who it's for

- Developers juggling multiple AI coding sessions at once
- Anyone wanting a glanceable "is this agent still productive?" signal
- Operators running long agent workflows who need to catch stalls and cost
  spikes before they get expensive

## What it deliberately isn't

agentop is a **read-only inspector**. It doesn't auto-approve tool calls, run
a local LLM gate, enforce spend budgets, or install hooks into your Claude /
Codex configuration. If you want those, projects like
[claudectl](https://github.com/mercurialsolo/claudectl) exist in that
adjacent space. agentop stays simple on purpose.

## Where to go next

- [Installation](installation.md) — pick your package manager
- [Quickstart](quickstart.md) — the first five minutes
- [Privacy and security](privacy-and-security.md) — what the tool reads and what it doesn't
