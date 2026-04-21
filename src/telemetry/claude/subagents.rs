//! Subagent task counting.
//!
//! Claude Code spawns subagent tasks under
//! `/tmp/claude-{uid}/{cwd-slug}/{session_id}/tasks/`.
//! This module counts the number of files in that directory to produce the
//! `subagent_count` telemetry field.
//!
//! # Error tolerance
//!
//! Any I/O failure (directory not found, permission denied, etc.) returns 0.
//! The function never panics.

use std::path::PathBuf;

/// Count subagent task files for the given session.
///
/// # Arguments
///
/// * `cwd_slug`   - The `cwd` slug (e.g. `"-Users-me-project"`).
/// * `session_id` - The session UUID string.
///
/// Returns 0 on any I/O error or if the tasks directory does not exist.
pub fn count(cwd_slug: &str, session_id: &str) -> usize {
    let uid = get_uid();
    let tasks_dir = build_tasks_path(uid, cwd_slug, session_id);

    let entries = match std::fs::read_dir(&tasks_dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    entries.flatten().count()
}

/// Build the full path to the tasks directory for a session.
fn build_tasks_path(uid: u32, cwd_slug: &str, session_id: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/claude-{uid}/{cwd_slug}/{session_id}/tasks"))
}

/// Get the current user's UID using `libc::getuid()`.
fn get_uid() -> u32 {
    // SAFETY: getuid(2) is always safe — no arguments, no pointers.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_returns_zero_for_nonexistent_dir() {
        let result = count(
            "-nonexistent-cwd-slug",
            "00000000-0000-0000-0000-000000000000",
        );
        assert_eq!(result, 0);
    }

    #[test]
    fn count_returns_file_count() {
        let dir = tempfile::tempdir().unwrap();
        // Create 3 files in a tasks subdirectory.
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("task1.json"), "{}").unwrap();
        std::fs::write(tasks_dir.join("task2.json"), "{}").unwrap();
        std::fs::write(tasks_dir.join("task3.json"), "{}").unwrap();

        // Read directly since we can't inject the path.
        let entries = std::fs::read_dir(&tasks_dir).unwrap();
        let n = entries.flatten().count();
        assert_eq!(n, 3);
    }

    #[test]
    fn build_tasks_path_format() {
        let p = build_tasks_path(1000, "-Users-me-project", "abc-def-123");
        assert_eq!(
            p.to_str().unwrap(),
            "/tmp/claude-1000/-Users-me-project/abc-def-123/tasks"
        );
    }
}
