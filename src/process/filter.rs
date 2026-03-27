use super::info::ProcessInfo;

/// Identifies whether a process belongs to Claude Code or Codex CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessKind {
    /// Anthropic Claude Code process.
    Claude,
    /// OpenAI Codex CLI process.
    Codex,
}

/// Returns `true` when `info` represents a Claude Code process.
///
/// Matches on:
/// - exact name `"claude"`,
/// - exe path containing `.local/share/claude`, or
/// - version-number binary name (digits/dots, len ≥ 3) under a `claude/versions` directory.
pub fn is_claude_process(info: &ProcessInfo) -> bool {
    // sysinfo reports "node" as the process name even when process.title = "claude",
    // so we also check argv[0] which carries the display name.
    let argv0 = info.cmd.first().map(|s| s.as_str()).unwrap_or("");
    if info.name == "claude" || argv0 == "claude" {
        return true;
    }
    let exe = info.exe_path.as_deref().unwrap_or("");
    if exe.contains(".local/share/claude") {
        return true;
    }
    let is_version_name =
        info.name.len() >= 3 && info.name.chars().all(|c| c.is_ascii_digit() || c == '.');
    is_version_name && exe.contains("claude/versions")
}

/// Returns `true` when `info` represents an OpenAI Codex CLI process.
///
/// Matches on:
/// - exact name `"codex"`, or
/// - any argv token containing `"@openai/codex"` or `"codex.js"`.
pub fn is_codex_process(info: &ProcessInfo) -> bool {
    let argv0 = info.cmd.first().map(|s| s.as_str()).unwrap_or("");
    if info.name == "codex" || argv0 == "codex" {
        return true;
    }
    info.cmd
        .iter()
        .any(|arg| arg.contains("@openai/codex") || arg.contains("codex.js"))
}

/// Returns `true` when the process is either a Claude or Codex process.
pub fn is_target_process(info: &ProcessInfo) -> bool {
    is_claude_process(info) || is_codex_process(info)
}

/// Returns the [`ProcessKind`] for `info`, or `None` if it is not a target process.
pub fn process_kind(info: &ProcessInfo) -> Option<ProcessKind> {
    if is_claude_process(info) {
        Some(ProcessKind::Claude)
    } else if is_codex_process(info) {
        Some(ProcessKind::Codex)
    } else {
        None
    }
}
