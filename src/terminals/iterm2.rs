// Structure inspired by mercurialsolo/claudectl (MIT, 2025-2026).
//! iTerm2 terminal adapter (macOS).
//!
//! Uses AppleScript via `osascript` to iterate iTerm2 sessions and activate
//! the one whose tty path matches the target process.

use std::process::Command;

use super::{TerminalAdapter, TerminalTarget};

/// Adapter for iTerm2 on macOS.
pub struct Iterm2Adapter;

impl TerminalAdapter for Iterm2Adapter {
    fn name(&self) -> &'static str {
        "iterm2"
    }

    fn current_target(&self) -> Option<TerminalTarget> {
        // Ask iTerm2 for the tty of the current session.
        let script = r#"tell application "iTerm2"
    tell current session of current window
        tty
    end tell
end tell"#;
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if tty.is_empty() {
            return None;
        }
        Some(TerminalTarget {
            terminal: "iterm2",
            pane_id: tty,
        })
    }

    fn focus_by_tty(&self, tty: &str) -> Result<String, String> {
        // Iterate all iTerm2 windows/tabs/sessions looking for a matching tty,
        // then activate that session and its window.
        let script = format!(
            r#"tell application "iTerm2"
    set targetTTY to "{tty}"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                if tty of s is equal to targetTTY then
                    select s
                    select t
                    set index of w to 1
                    return tty of s
                end if
            end repeat
        end repeat
    end repeat
    return ""
end tell"#,
        );

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("osascript not in PATH: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("osascript failed: {stderr}"));
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if result.is_empty() {
            Err(format!("no iTerm2 session found for tty {tty}"))
        } else {
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_name() {
        assert_eq!(Iterm2Adapter.name(), "iterm2");
    }
}
