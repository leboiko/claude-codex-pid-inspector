// Structure inspired by mercurialsolo/claudectl (MIT, 2025-2026).
//! Terminal adapter layer for the "jump to terminal pane" feature (Phase 4).
//!
//! Provides a [`TerminalAdapter`] trait and concrete implementations for
//! tmux, iTerm2 (macOS), and Kitty. WezTerm is detected but not yet
//! implemented — it returns a descriptive error instead of silently failing.
//!
//! # Detection order
//!
//! [`detect`] probes environment variables in this order:
//! 1. `TMUX` — tmux
//! 2. `KITTY_WINDOW_ID` — Kitty
//! 3. `TERM_PROGRAM=WezTerm` — WezTerm (stub)
//! 4. `TERM_PROGRAM=iTerm.app` (macOS only) — iTerm2
//!
//! The first match wins and is returned as a boxed adapter.

pub mod iterm2;
pub mod kitty;
pub mod tmux;
pub mod wezterm;

/// A terminal pane or window address usable for display in status messages.
///
/// Contains the terminal name and a human-readable pane identifier.
#[derive(Debug, Clone)]
pub struct TerminalTarget {
    /// Short terminal name, e.g. `"tmux"`.
    pub terminal: &'static str,
    /// Pane or window id rendered in status messages.
    pub pane_id: String,
}

/// Trait implemented by each terminal adapter.
///
/// Adapters use `std::process::Command` to query and focus terminal panes.
/// They MUST NOT panic — errors are returned as `Err(String)` so the caller
/// can show a user-facing status message.
pub trait TerminalAdapter: Send {
    /// Short, stable name shown in flash messages, e.g. `"tmux"`.
    fn name(&self) -> &'static str;

    /// Return a [`TerminalTarget`] describing the terminal pane that currently
    /// runs this process of `agentop`, or `None` if the query fails.
    fn current_target(&self) -> Option<TerminalTarget>;

    /// Focus the pane that owns `tty` (a TTY device path like `"/dev/pts/3"`).
    ///
    /// On success returns a short description of what was focused, e.g.
    /// `"window:1.0"`. On failure returns a human-readable error string.
    fn focus_by_tty(&self, tty: &str) -> Result<String, String>;
}

/// Probe the environment and return the best-matching [`TerminalAdapter`].
///
/// Returns `None` when running in an unsupported terminal, which tells the
/// caller to show the "terminal not detected" flash message.
///
/// # Arguments
///
/// * `env` - Environment variable map; accepts any `Fn(&str) -> Option<String>`
///   so tests can inject a mock environment instead of reading the real one.
pub fn detect<F>(env: F) -> Option<Box<dyn TerminalAdapter>>
where
    F: Fn(&str) -> Option<String>,
{
    // tmux: $TMUX is set to a socket path.
    if env("TMUX").is_some() {
        return Some(Box::new(tmux::TmuxAdapter));
    }

    // Kitty: $KITTY_WINDOW_ID is the window id.
    if env("KITTY_WINDOW_ID").is_some() {
        return Some(Box::new(kitty::KittyAdapter));
    }

    // WezTerm: TERM_PROGRAM=WezTerm.
    if env("TERM_PROGRAM").as_deref() == Some("WezTerm") {
        return Some(Box::new(wezterm::WeztermAdapter));
    }

    // iTerm2: TERM_PROGRAM=iTerm.app (macOS only, but the env var check is
    // cross-platform so CI on Linux stays clean).
    if env("TERM_PROGRAM").as_deref() == Some("iTerm.app") {
        return Some(Box::new(iterm2::Iterm2Adapter));
    }

    None
}

/// Convenience wrapper that reads from the real process environment.
pub fn detect_from_env() -> Option<Box<dyn TerminalAdapter>> {
    detect(|key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |key| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn detects_tmux() {
        let env = mock_env(&[("TMUX", "/tmp/tmux-1000/default,12345,0")]);
        let adapter = detect(env).expect("should detect tmux");
        assert_eq!(adapter.name(), "tmux");
    }

    #[test]
    fn detects_kitty() {
        let env = mock_env(&[("KITTY_WINDOW_ID", "1")]);
        let adapter = detect(env).expect("should detect kitty");
        assert_eq!(adapter.name(), "kitty");
    }

    #[test]
    fn detects_wezterm() {
        let env = mock_env(&[("TERM_PROGRAM", "WezTerm")]);
        let adapter = detect(env).expect("should detect wezterm");
        assert_eq!(adapter.name(), "wezterm");
    }

    #[test]
    fn detects_iterm2() {
        let env = mock_env(&[("TERM_PROGRAM", "iTerm.app")]);
        let adapter = detect(env).expect("should detect iterm2");
        assert_eq!(adapter.name(), "iterm2");
    }

    #[test]
    fn unknown_terminal_returns_none() {
        let env = mock_env(&[]);
        assert!(detect(env).is_none());
    }

    #[test]
    fn tmux_takes_priority_over_iterm2() {
        // Both set — tmux wins because it is checked first.
        let env = mock_env(&[
            ("TMUX", "/tmp/tmux-1000/default,12345,0"),
            ("TERM_PROGRAM", "iTerm.app"),
        ]);
        let adapter = detect(env).expect("should detect");
        assert_eq!(adapter.name(), "tmux");
    }
}
