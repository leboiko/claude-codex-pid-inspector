// Structure inspired by mercurialsolo/claudectl (MIT, 2025-2026).
//! WezTerm terminal adapter (stub).
//!
//! WezTerm is detected via `TERM_PROGRAM=WezTerm` but pane-focus requires
//! querying `wezterm cli list` for the pane id that owns a given tty, which
//! needs more research on cross-platform pane-id stability.
//!
//! This stub returns a descriptive error rather than silently failing, so the
//! user sees a "not yet implemented" message instead of nothing.

use super::{TerminalAdapter, TerminalTarget};

/// Adapter for the WezTerm terminal emulator (detected but not fully implemented).
pub struct WeztermAdapter;

impl TerminalAdapter for WeztermAdapter {
    fn name(&self) -> &'static str {
        "wezterm"
    }

    fn current_target(&self) -> Option<TerminalTarget> {
        // WEZTERM_PANE is set by WezTerm to the current pane id.
        let pane_id = std::env::var("WEZTERM_PANE").ok()?;
        Some(TerminalTarget {
            terminal: "wezterm",
            pane_id,
        })
    }

    fn focus_by_tty(&self, _tty: &str) -> Result<String, String> {
        Err(
            "wezterm jump not yet implemented — pane-id lookup via tty requires further research"
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_name() {
        assert_eq!(WeztermAdapter.name(), "wezterm");
    }

    #[test]
    fn focus_returns_err() {
        let err = WeztermAdapter.focus_by_tty("/dev/pts/1");
        assert!(err.is_err());
    }
}
