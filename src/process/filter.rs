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

/// Returns the user-facing display name for a process.
///
/// Claude Code and Codex CLI are Node.js apps, so `sysinfo` reports their
/// process name as `"node"`. This helper returns the friendly name
/// (`"claude"` / `"codex"`) for detected target processes, falling back to
/// the raw OS name for everything else.
pub fn display_name(info: &ProcessInfo) -> &str {
    match process_kind(info) {
        Some(ProcessKind::Claude) => "claude",
        Some(ProcessKind::Codex) => "codex",
        None => &info.name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessInfo;

    fn make_info(name: &str, cmd: Vec<&str>, exe: Option<&str>) -> ProcessInfo {
        ProcessInfo {
            pid: 1,
            parent_pid: None,
            name: name.to_string(),
            cmd: cmd.into_iter().map(String::from).collect(),
            exe_path: exe.map(String::from),
            cwd: None,
            cpu_usage: 0.0,
            memory_bytes: 0,
            status: "Run".to_string(),
            environ_count: 0,
            start_time: 0,
            run_time: 0,
        }
    }

    #[test]
    fn claude_by_name() {
        let info = make_info("claude", vec!["claude"], None);
        assert!(is_claude_process(&info));
        assert!(is_target_process(&info));
    }

    #[test]
    fn claude_by_argv0() {
        let info = make_info("node", vec!["claude"], None);
        assert!(is_claude_process(&info));
    }

    #[test]
    fn claude_by_exe_path() {
        let info = make_info(
            "node",
            vec!["node"],
            Some("/home/user/.local/share/claude/bin/claude"),
        );
        assert!(is_claude_process(&info));
    }

    #[test]
    fn claude_by_version_name() {
        let info = make_info(
            "2.1.85",
            vec![],
            Some("/home/user/.local/share/claude/versions/2.1.85"),
        );
        assert!(is_claude_process(&info));
    }

    #[test]
    fn not_claude_random_process() {
        let info = make_info("firefox", vec!["firefox"], Some("/usr/bin/firefox"));
        assert!(!is_claude_process(&info));
    }

    #[test]
    fn codex_by_name() {
        let info = make_info("codex", vec!["codex"], None);
        assert!(is_codex_process(&info));
        assert!(is_target_process(&info));
    }

    #[test]
    fn codex_by_argv0() {
        let info = make_info("node", vec!["codex"], None);
        assert!(is_codex_process(&info));
    }

    #[test]
    fn codex_by_cmd_openai() {
        let info = make_info("node", vec!["node", "/path/to/@openai/codex/bin"], None);
        assert!(is_codex_process(&info));
    }

    #[test]
    fn codex_by_cmd_js() {
        let info = make_info("node", vec!["node", "codex.js"], None);
        assert!(is_codex_process(&info));
    }

    #[test]
    fn process_kind_detection() {
        let claude = make_info("claude", vec!["claude"], None);
        assert_eq!(process_kind(&claude), Some(ProcessKind::Claude));

        let codex = make_info("codex", vec!["codex"], None);
        assert_eq!(process_kind(&codex), Some(ProcessKind::Codex));

        let other = make_info("bash", vec!["bash"], None);
        assert_eq!(process_kind(&other), None);
    }
}
