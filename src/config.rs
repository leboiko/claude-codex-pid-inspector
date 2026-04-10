//! Persistent user settings.
//!
//! The config file lives at `$XDG_CONFIG_HOME/agentop/config.toml`
//! (`~/Library/Application Support/agentop/config.toml` on macOS, thanks to
//! the `dirs` crate which returns the platform-native config root). It is
//! written whenever the user applies a new setting in the settings popup,
//! and read once on startup.
//!
//! All I/O failures are swallowed silently: a TUI has no reasonable place to
//! log them, and a missing or malformed config file should never prevent the
//! app from starting. On a parse error, the defaults are used.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::styles::{GraphStyle, Theme};

/// Subdirectory name under the platform config root.
const APP_NAME: &str = "agentop";
/// Filename for the serialized config within the app dir.
const CONFIG_FILE: &str = "config.toml";

/// All persisted settings. `#[serde(default)]` on every field means an older
/// config file missing a newly-introduced field will load successfully and
/// fall back to the type default, so forward compatibility is free.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub graph_style: GraphStyle,
}

impl Config {
    /// Load the config from disk, returning defaults on any failure.
    ///
    /// This is deliberately infallible from the caller's point of view:
    /// - no file → default config,
    /// - unreadable file → default config,
    /// - malformed TOML → default config.
    ///
    /// A fresh default is also returned if the platform has no config dir.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Persist the config to disk. Silently ignores any I/O error — if the
    /// config can't be saved, the app still runs normally with the current
    /// in-memory settings.
    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };
        // Ensure the parent directory exists; `create_dir_all` is a no-op if
        // it already does.
        if let Some(parent) = path.parent() {
            if fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let Ok(text) = toml::to_string_pretty(self) else {
            return;
        };
        let _ = fs::write(&path, text);
    }
}

/// Resolve the full path to the config file, or `None` if the platform does
/// not expose a config directory.
fn config_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push(APP_NAME);
    path.push(CONFIG_FILE);
    Some(path)
}
