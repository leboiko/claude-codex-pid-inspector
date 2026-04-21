// Structure inspired by mercurialsolo/claudectl (MIT, 2025-2026).
//! Kitty terminal adapter.
//!
//! Uses `kitten @ ls` (the Kitty remote-control API) to list windows and find
//! the one whose tty matches, then focuses it with `kitten @ focus-window`.

use std::process::Command;

use super::{TerminalAdapter, TerminalTarget};

/// Adapter for the Kitty terminal emulator.
pub struct KittyAdapter;

impl TerminalAdapter for KittyAdapter {
    fn name(&self) -> &'static str {
        "kitty"
    }

    fn current_target(&self) -> Option<TerminalTarget> {
        // `$KITTY_WINDOW_ID` is set by Kitty to the current window id.
        let window_id = std::env::var("KITTY_WINDOW_ID").ok()?;
        Some(TerminalTarget {
            terminal: "kitty",
            pane_id: window_id,
        })
    }

    fn focus_by_tty(&self, tty: &str) -> Result<String, String> {
        // `kitten @ focus-window --match tty:<tty>` directly focuses the window.
        let tty_match = format!("tty:{tty}");
        let output = Command::new("kitten")
            .args(["@", "focus-window", "--match", &tty_match])
            .output()
            .map_err(|e| format!("kitten not in PATH: {e}"))?;

        if output.status.success() {
            return Ok(tty.to_string());
        }

        // kitten @ focus-window may require the kitty remote-control socket.
        // Try with the socket path if KITTY_LISTEN_ON is set.
        if let Ok(listen_on) = std::env::var("KITTY_LISTEN_ON") {
            let output2 = Command::new("kitten")
                .args([
                    "@",
                    "--to",
                    &listen_on,
                    "focus-window",
                    "--match",
                    &tty_match,
                ])
                .output()
                .map_err(|e| format!("kitten not in PATH: {e}"))?;

            if output2.status.success() {
                return Ok(tty.to_string());
            }
            let stderr = String::from_utf8_lossy(&output2.stderr);
            return Err(format!("kitten focus-window failed: {stderr}"));
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("kitten focus-window failed: {stderr}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_name() {
        assert_eq!(KittyAdapter.name(), "kitty");
    }
}
