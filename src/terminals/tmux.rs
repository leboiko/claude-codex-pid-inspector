// Structure inspired by mercurialsolo/claudectl (MIT, 2025-2026).
//! tmux terminal adapter.
//!
//! Uses `tmux list-panes -a -F` to enumerate all panes and find the one whose
//! tty matches the target process's controlling terminal, then calls
//! `tmux select-window` and `tmux select-pane` to bring it into focus.

use std::process::Command;

use super::{TerminalAdapter, TerminalTarget};

/// Adapter for tmux sessions.
pub struct TmuxAdapter;

impl TerminalAdapter for TmuxAdapter {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn current_target(&self) -> Option<TerminalTarget> {
        // `tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}'`
        let output = Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "#{session_name}:#{window_index}.#{pane_index}",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if pane_id.is_empty() {
            return None;
        }
        Some(TerminalTarget {
            terminal: "tmux",
            pane_id,
        })
    }

    fn focus_by_tty(&self, tty: &str) -> Result<String, String> {
        // List all panes with their tty path.
        // Format: session_name:window_index.pane_index tty
        let output = Command::new("tmux")
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name}:#{window_index}.#{pane_index} #{pane_tty}",
            ])
            .output()
            .map_err(|e| format!("tmux not in PATH: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tmux list-panes failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Normalise tty: strip leading /dev/ if present for comparison.
        let tty_norm = tty.trim_start_matches("/dev/");

        let target = stdout.lines().find_map(|line| {
            let mut parts = line.splitn(2, ' ');
            let pane = parts.next()?;
            let pane_tty = parts.next()?.trim();
            let pane_tty_norm = pane_tty.trim_start_matches("/dev/");
            if pane_tty_norm == tty_norm {
                Some(pane.to_string())
            } else {
                None
            }
        });

        let pane = target.ok_or_else(|| format!("no tmux pane found for tty {tty}"))?;

        // Focus: select the window first, then the pane.
        // `pane` is in the form `session:window.pane`.
        let select_output = Command::new("tmux")
            .args(["select-pane", "-t", &pane])
            .output()
            .map_err(|e| format!("tmux select-pane failed: {e}"))?;

        if !select_output.status.success() {
            let stderr = String::from_utf8_lossy(&select_output.stderr);
            return Err(format!("tmux select-pane failed: {stderr}"));
        }

        // Also switch to the window so it becomes visible.
        // Extract the window target (everything before the last `.`).
        let window_target = pane
            .rsplit_once('.')
            .map(|(w, _)| w)
            .unwrap_or(pane.as_str());
        let _ = Command::new("tmux")
            .args(["select-window", "-t", window_target])
            .output();

        Ok(pane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_name() {
        assert_eq!(TmuxAdapter.name(), "tmux");
    }
}
