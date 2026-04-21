//! Session index — maps PIDs to Claude session metadata.
//!
//! [`SessionIndex`] scans `~/.claude/sessions/*.json` and caches the result
//! with a 30-second TTL. A PID miss triggers an immediate re-scan regardless
//! of the TTL, because a new Claude session might have started since the last
//! scan.
//!
//! # Error tolerance
//!
//! - If the sessions directory does not exist the index remains empty.
//! - Files that fail to parse are skipped with a debug log.
//! - I/O errors during directory traversal are silently ignored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::schema::SessionMeta;

/// How long the session index cache is considered fresh before re-scanning.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// Cached mapping from PID to session metadata.
///
/// Rebuilt by scanning `~/.claude/sessions/*.json` at most once per
/// 30 seconds (see `CACHE_TTL`), or sooner if a PID miss forces an
/// immediate refresh.
#[derive(Debug, Default)]
pub struct SessionIndex {
    /// The cached mapping.
    sessions: HashMap<u32, SessionMeta>,
    /// When the cache was last populated.
    last_scan: Option<Instant>,
    /// The sessions directory, resolved once at construction time.
    sessions_dir: Option<PathBuf>,
}

impl SessionIndex {
    /// Create a new index, resolving `~/.claude/sessions/` via the `dirs` crate.
    pub fn new() -> Self {
        let sessions_dir = dirs::home_dir().map(|h| h.join(".claude").join("sessions"));
        Self {
            sessions: HashMap::new(),
            last_scan: None,
            sessions_dir,
        }
    }

    /// Look up session metadata for the given PID.
    ///
    /// Refreshes the cache if:
    /// - The cache has never been populated.
    /// - The cache is older than 30 seconds.
    /// - The PID is not in the cache (may be a brand-new session).
    ///
    /// Returns `None` if no matching session file exists.
    pub fn get(&mut self, pid: u32) -> Option<&SessionMeta> {
        let needs_scan = self
            .last_scan
            .map(|t| t.elapsed() > CACHE_TTL)
            .unwrap_or(true);

        if needs_scan {
            self.scan();
        }

        if !self.sessions.contains_key(&pid) && !needs_scan {
            // PID miss after a fresh cache — force one more scan.
            self.scan();
        }

        self.sessions.get(&pid)
    }

    /// Prune sessions for PIDs that are no longer in `live_pids`.
    ///
    /// Called on each telemetry tick to prevent unbounded memory growth.
    pub fn prune(&mut self, live_pids: &[u32]) {
        let live: std::collections::HashSet<u32> = live_pids.iter().copied().collect();
        self.sessions.retain(|pid, _| live.contains(pid));
    }

    /// Scan the sessions directory and repopulate the cache.
    fn scan(&mut self) {
        self.last_scan = Some(Instant::now());

        let dir = match &self.sessions_dir {
            Some(d) => d.clone(),
            None => return,
        };

        if !dir.exists() {
            return;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut new_sessions: HashMap<u32, SessionMeta> = HashMap::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(meta) = parse_session_file(&path) {
                new_sessions.insert(meta.pid, meta);
            }
        }

        self.sessions = new_sessions;
    }

    /// Return `true` when the sessions directory exists on this machine.
    ///
    /// Used in `main.rs` to decide whether to register the Claude provider.
    pub fn sessions_dir_exists(&self) -> bool {
        self.sessions_dir
            .as_deref()
            .map(Path::exists)
            .unwrap_or(false)
    }

    /// Override the sessions directory. Used in tests to inject a synthetic tree.
    pub fn set_sessions_dir(&mut self, dir: PathBuf) {
        self.sessions_dir = Some(dir);
        // Invalidate cache so the new directory is scanned on next access.
        self.last_scan = None;
        self.sessions.clear();
    }
}

/// Parse a single `{pid}.json` session metadata file.
///
/// Returns `None` on any I/O or parse error.
fn parse_session_file(path: &Path) -> Option<SessionMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<SessionMeta>(&content)
        .map_err(|e| {
            debug_log(&format!(
                "SessionIndex: failed to parse {:?}: {e}",
                path.file_name()
            ));
        })
        .ok()
}

/// Compute the `cwd-slug` used in transcript path construction.
///
/// Claude replaces each `/` in the cwd string with `-`. Because an absolute
/// path starts with `/`, the slug starts with `-`.
///
/// # Examples
///
/// ```
/// use agentop::telemetry::claude::discovery::cwd_to_slug;
/// assert_eq!(cwd_to_slug("/Users/me/project"), "-Users-me-project");
/// ```
pub fn cwd_to_slug(cwd: &str) -> String {
    cwd.replace('/', "-")
}

/// Emit a debug message to stderr when `AGENTOP_DEBUG=1`.
fn debug_log(msg: &str) {
    if std::env::var("AGENTOP_DEBUG").as_deref() == Ok("1") {
        eprintln!("[agentop] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_to_slug_replaces_slashes() {
        assert_eq!(cwd_to_slug("/Users/alice/project"), "-Users-alice-project");
    }

    #[test]
    fn cwd_to_slug_root() {
        assert_eq!(cwd_to_slug("/"), "-");
    }

    #[test]
    fn cwd_to_slug_no_slashes() {
        assert_eq!(cwd_to_slug("project"), "project");
    }

    #[test]
    fn session_index_empty_on_nonexistent_dir() {
        // Create an index pointing at a directory that doesn't exist.
        let mut idx = SessionIndex {
            sessions: HashMap::new(),
            last_scan: None,
            sessions_dir: Some(PathBuf::from("/nonexistent/path/that/does/not/exist")),
        };
        // Should return None gracefully.
        assert!(idx.get(1234).is_none());
    }

    #[test]
    fn session_index_parses_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let meta = serde_json::json!({
            "pid": 42,
            "sessionId": "test-session-id",
            "cwd": "/Users/test/project",
            "startedAt": 1_700_000_000_000u64,
            "version": "2.1.116",
            "peerProtocol": 1,
            "kind": "interactive",
            "entrypoint": "cli"
        });
        std::fs::write(
            dir.path().join("42.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let mut idx = SessionIndex {
            sessions: HashMap::new(),
            last_scan: None,
            sessions_dir: Some(dir.path().to_path_buf()),
        };

        let result = idx.get(42);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.pid, 42);
        assert_eq!(m.session_id, "test-session-id");
        assert_eq!(m.cwd, "/Users/test/project");
    }

    #[test]
    fn session_index_skips_non_json_files() {
        let dir = tempfile::tempdir().unwrap();
        // Write a non-JSON file that would cause a panic if parsed.
        std::fs::write(dir.path().join("README.txt"), "not a session file").unwrap();

        let mut idx = SessionIndex {
            sessions: HashMap::new(),
            last_scan: None,
            sessions_dir: Some(dir.path().to_path_buf()),
        };
        // Should not panic.
        assert!(idx.get(99).is_none());
    }

    #[test]
    fn session_index_skips_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.json"), "{not valid json").unwrap();

        let mut idx = SessionIndex {
            sessions: HashMap::new(),
            last_scan: None,
            sessions_dir: Some(dir.path().to_path_buf()),
        };
        assert!(idx.get(1).is_none());
    }

    #[test]
    fn prune_removes_stale_pids() {
        let dir = tempfile::tempdir().unwrap();
        let meta = serde_json::json!({"pid": 10, "sessionId": "s1", "cwd": "/a", "startedAt": 0, "version": "", "peerProtocol": 1, "kind": "", "entrypoint": ""});
        std::fs::write(
            dir.path().join("10.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let mut idx = SessionIndex {
            sessions: HashMap::new(),
            last_scan: None,
            sessions_dir: Some(dir.path().to_path_buf()),
        };
        idx.get(10); // populate cache
        assert!(idx.sessions.contains_key(&10));

        idx.prune(&[99]); // PID 10 not in live list
        assert!(!idx.sessions.contains_key(&10));
    }
}
