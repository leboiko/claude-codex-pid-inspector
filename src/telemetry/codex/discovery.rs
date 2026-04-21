//! CODEX_HOME resolution and rollout file discovery.
//!
//! # Overview
//!
//! Unlike Claude Code, Codex does not write a pidfile. PID-to-session
//! correlation is done by:
//!
//! 1. Resolving the `CODEX_HOME` directory for the target PID by reading its
//!    process environment.
//! 2. Locating the rollout file whose filename timestamp is within ±90 seconds
//!    of the process start time.
//!
//! ## CODEX_HOME resolution
//!
//! - **macOS**: Run `ps eww -p <pid> -o command=` — the `eww` flag appends the
//!   environment to the command line, separated by spaces. Parse for `CODEX_HOME=<value>`.
//! - **Linux**: Read `/proc/{pid}/environ` (null-separated byte stream) and
//!   find the `CODEX_HOME=` entry.
//! - **Fallback**: `dirs::home_dir()?.join(".codex")`.
//!
//! The resolved value is cached per PID; environment variables do not change
//! mid-process.
//!
//! ## Rollout file location
//!
//! Walk `{codex_home}/sessions/{today_utc_yyyy}/{today_utc_mm}/{today_utc_dd}/`
//! and the previous-day directory (to cover UTC rollovers). Filter to
//! `rollout-*.jsonl` files whose filename timestamp is within ±90 seconds of
//! the process start time. Among matching files, pick the one with the most
//! recent `mtime`.
//!
//! # Error tolerance
//!
//! All errors degrade gracefully: unreadable process environments fall back to
//! the default `~/.codex`; missing directories produce no rollout file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// ±90 seconds tolerance for filename timestamp vs. process start time.
const TIMESTAMP_TOLERANCE_SECS: i64 = 90;

/// After this many consecutive ticks with no mtime advance, re-resolve the
/// rollout file (covers rare session rotation within the same process).
const MTIME_STALE_TICKS: u8 = 3;

/// Per-PID cache entry for the resolved CODEX_HOME path.
#[derive(Debug)]
struct HomeEntry {
    path: PathBuf,
}

/// Resolves and caches `CODEX_HOME` for PIDs seen in the current scan.
///
/// The resolved path is cached indefinitely for each PID because environment
/// variables do not change while a process is running.
pub struct CodexHomeResolver {
    /// Per-PID cached CODEX_HOME paths.
    cache: HashMap<u32, HomeEntry>,
    /// Default `~/.codex` path, computed once at construction.
    ///
    /// Exposed as `pub` so tests and [`crate::telemetry::codex::provider::CodexTelemetryProvider::from_codex_home`]
    /// can override it with a synthetic tree.
    pub default_home: Option<PathBuf>,
}

impl CodexHomeResolver {
    /// Create a new resolver. Resolves the default `~/.codex` path immediately.
    pub fn new() -> Self {
        let default_home = dirs::home_dir().map(|h| h.join(".codex"));
        Self {
            cache: HashMap::new(),
            default_home,
        }
    }

    /// Resolve the `CODEX_HOME` for `pid`, returning the cached value or
    /// performing a fresh resolution.
    ///
    /// Returns the default `~/.codex` if the process environment is unreadable
    /// or does not set `CODEX_HOME`.
    pub fn resolve(&mut self, pid: u32) -> Option<PathBuf> {
        if let Some(entry) = self.cache.get(&pid) {
            return Some(entry.path.clone());
        }
        let path = self.resolve_uncached(pid);
        if let Some(ref p) = path {
            self.cache.insert(pid, HomeEntry { path: p.clone() });
        }
        path
    }

    /// Prune cached entries for PIDs that are no longer running.
    pub fn prune(&mut self, live_pids: &[u32]) {
        let live: std::collections::HashSet<u32> = live_pids.iter().copied().collect();
        self.cache.retain(|pid, _| live.contains(pid));
    }

    /// Return `true` when the default `~/.codex/sessions/` directory exists.
    ///
    /// Used in `main.rs` to decide whether to register the Codex provider.
    pub fn default_sessions_dir_exists(&self) -> bool {
        self.default_home
            .as_deref()
            .map(|h| h.join("sessions").exists())
            .unwrap_or(false)
    }

    /// Perform a fresh CODEX_HOME resolution for the given PID.
    fn resolve_uncached(&self, pid: u32) -> Option<PathBuf> {
        if let Some(path) = read_codex_home_from_env(pid) {
            return Some(PathBuf::from(path));
        }
        self.default_home.clone()
    }
}

impl Default for CodexHomeResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Read the `CODEX_HOME` value from the process environment for `pid`.
///
/// Returns `None` if the variable is not set or the environment cannot be read.
///
/// # Platform behaviour
///
/// - **macOS**: Invokes `ps eww -p <pid> -o command=`. With the `eww` flag, ps
///   appends the full environment block to the command string. We scan for
///   `CODEX_HOME=` in the output.
/// - **Linux**: Reads `/proc/{pid}/environ`, a null-byte-separated list of
///   `KEY=VALUE` pairs.
/// - **Other**: Returns `None` immediately.
fn read_codex_home_from_env(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        read_codex_home_macos(pid)
    }
    #[cfg(target_os = "linux")]
    {
        read_codex_home_linux(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

/// macOS implementation: parse `ps eww -p <pid> -o command=` output.
#[cfg(target_os = "macos")]
fn read_codex_home_macos(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["eww", "-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    extract_codex_home_from_env_string(&text)
}

/// Linux implementation: parse `/proc/{pid}/environ`.
#[cfg(target_os = "linux")]
fn read_codex_home_linux(pid: u32) -> Option<String> {
    let path = format!("/proc/{pid}/environ");
    let data = std::fs::read(&path).ok()?;
    // Each entry is NUL-terminated.
    for entry in data.split(|&b| b == 0) {
        if let Ok(s) = std::str::from_utf8(entry) {
            if let Some(value) = s.strip_prefix("CODEX_HOME=") {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract the value of `CODEX_HOME` from a space-separated environment string.
///
/// Used for macOS `ps eww` output where environment variables are appended
/// after the command line as space-separated `KEY=VALUE` tokens.
fn extract_codex_home_from_env_string(s: &str) -> Option<String> {
    // The environment block is appended after a space following the argv.
    // Scan for the CODEX_HOME token.
    for token in s.split_whitespace() {
        if let Some(value) = token.strip_prefix("CODEX_HOME=") {
            return Some(value.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rollout file locator
// ---------------------------------------------------------------------------

/// Per-PID cached rollout file state.
#[derive(Debug)]
struct RolloutEntry {
    /// Resolved path to the active rollout file.
    path: PathBuf,
    /// Last-known mtime of the file.
    last_mtime: SystemTime,
    /// Consecutive ticks with no mtime advance.
    stale_ticks: u8,
}

/// Locates the active rollout file for a PID and caches the result.
///
/// The cache is invalidated per-PID when the cached file's mtime has not
/// advanced for several consecutive ticks, which covers the rare case of
/// in-process session rotation.
pub struct RolloutLocator {
    /// Per-PID cached rollout file state.
    cache: HashMap<u32, RolloutEntry>,
}

impl RolloutLocator {
    /// Create a new locator with an empty cache.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Find the active rollout file for `pid` given its `codex_home` and
    /// `process_start_secs` (Unix epoch seconds).
    ///
    /// Returns the resolved path, or `None` if no matching file can be found.
    pub fn locate(
        &mut self,
        pid: u32,
        codex_home: &Path,
        process_start_secs: u64,
    ) -> Option<PathBuf> {
        // Check cached entry.
        if let Some(entry) = self.cache.get_mut(&pid) {
            let current_mtime = std::fs::metadata(&entry.path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);

            if current_mtime > entry.last_mtime {
                // mtime advanced — file is still active.
                entry.last_mtime = current_mtime;
                entry.stale_ticks = 0;
                return Some(entry.path.clone());
            }

            entry.stale_ticks += 1;
            if entry.stale_ticks < MTIME_STALE_TICKS {
                // Not enough stale ticks to trigger re-resolution.
                return Some(entry.path.clone());
            }

            // Stale for too long — fall through to re-resolve.
            debug_log(&format!(
                "RolloutLocator: re-resolving for PID {pid} after stale mtime"
            ));
        }

        // Fresh resolution.
        let path = find_rollout_file(codex_home, process_start_secs)?;
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        self.cache.insert(
            pid,
            RolloutEntry {
                path: path.clone(),
                last_mtime: mtime,
                stale_ticks: 0,
            },
        );
        Some(path)
    }

    /// Prune cached entries for PIDs that are no longer running.
    pub fn prune(&mut self, live_pids: &[u32]) {
        let live: std::collections::HashSet<u32> = live_pids.iter().copied().collect();
        self.cache.retain(|pid, _| live.contains(pid));
    }
}

impl Default for RolloutLocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the rollout file in `codex_home/sessions/` matching `process_start_secs`.
///
/// Searches today's and yesterday's UTC date directories. Picks the matching
/// file with the most recent mtime.
pub fn find_rollout_file(codex_home: &Path, process_start_secs: u64) -> Option<PathBuf> {
    let sessions_root = codex_home.join("sessions");

    // Compute today and yesterday in UTC.
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(process_start_secs);
    // Use simple arithmetic: 86400 seconds per day.
    let start_secs = process_start_secs as i64;
    let today_secs = start_secs - (start_secs % 86400); // midnight UTC
    let yesterday_secs = today_secs - 86400;

    let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();

    for day_secs in [today_secs, yesterday_secs] {
        if let Some(dir) = utc_sessions_dir(&sessions_root, day_secs) {
            scan_dir_for_candidates(&dir, process_start_secs, &mut candidates);
        }
    }

    // Also check the next day (in case our wall clock lags).
    let tomorrow_secs = today_secs + 86400;
    if let Some(dir) = utc_sessions_dir(&sessions_root, tomorrow_secs) {
        scan_dir_for_candidates(&dir, process_start_secs, &mut candidates);
    }

    // Pick the candidate with the most recent mtime.
    let _ = start; // `start` was computed but not directly used; kept for documentation clarity.
    candidates
        .into_iter()
        .max_by_key(|(_, mtime)| *mtime)
        .map(|(path, _)| path)
}

/// Build the sessions day-directory path for a Unix timestamp (seconds).
///
/// Returns `None` if the directory does not exist.
fn utc_sessions_dir(sessions_root: &Path, day_secs: i64) -> Option<PathBuf> {
    if day_secs < 0 {
        return None;
    }
    let (y, m, d) = secs_to_ymd(day_secs as u64);
    let dir = sessions_root
        .join(format!("{y:04}"))
        .join(format!("{m:02}"))
        .join(format!("{d:02}"));

    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// Convert Unix seconds to (year, month, day) in UTC.
///
/// Uses a simple Gregorian calendar algorithm — no external dependencies.
fn secs_to_ymd(secs: u64) -> (u32, u32, u32) {
    let days = (secs / 86400) as u32;
    // Days since 1970-01-01 to year/month/day.
    // Algorithm: shift to a proleptic Gregorian epoch at 1 Mar 0000.
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Parse the filename timestamp from a rollout filename.
///
/// Expected format: `rollout-{YYYY}-{MM}-{DD}T{HH}-{MM}-{SS}-{uuid}.jsonl`
///
/// Returns the Unix epoch seconds of the timestamp, or `None` on parse failure.
pub fn parse_rollout_filename_secs(name: &str) -> Option<u64> {
    // Strip "rollout-" prefix.
    let rest = name.strip_prefix("rollout-")?;
    // Find the UUID part: after the 6th segment (date and time parts) there is a UUID.
    // Format: YYYY-MM-DDT HH-MM-SS-<uuid-first-group>-...
    // The timestamp portion is the first 19 chars: "YYYY-MM-DDTHH-MM-SS"
    // But colons are replaced by dashes, so we parse manually.
    // rest = "2026-04-21T10-43-38-019db048-405b-73f3-8523-bf79aca97ace.jsonl"
    let rest = rest.strip_suffix(".jsonl").unwrap_or(rest);

    // Split at 'T' to separate date and time parts.
    let t_pos = rest.find('T')?;
    let date_part = &rest[..t_pos]; // "2026-04-21"
    let after_t = &rest[t_pos + 1..]; // "10-43-38-019db048-..."

    // Date part: YYYY-MM-DD
    let mut date_parts = date_part.splitn(3, '-');
    let year: u32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;

    // Time part: HH-MM-SS followed by -<uuid>
    let mut time_parts = after_t.splitn(4, '-');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;

    // Convert to Unix epoch seconds via simple calculation.
    Some(ymd_hms_to_secs(year, month, day, hour, minute, second))
}

/// Convert year/month/day/hour/minute/second in UTC to Unix epoch seconds.
///
/// Used in tests to construct synthetic process start times.
pub fn ymd_hms_to_secs(y: u32, m: u32, d: u32, h: u32, min: u32, s: u32) -> u64 {
    // Days from 1970-01-01 to (y, m, d), then + time.
    let days = days_from_civil(y, m, d);
    (days as u64) * 86400 + h as u64 * 3600 + min as u64 * 60 + s as u64
}

/// Days from 1970-01-01 to a given Gregorian date.
///
/// Uses the proleptic Gregorian calendar algorithm from Howard Hinnant's
/// chrono date library (public domain).
fn days_from_civil(y: u32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (m as i64 + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Scan `dir` for rollout files matching `process_start_secs` within tolerance.
///
/// The filename timestamp cannot be trusted for correlation: Codex writes it
/// in the session's *local* time, so on a non-UTC machine the filename says
/// `10-43-38` while the process actually started at `13:43:38 UTC`. Instead,
/// we open each candidate and read the authoritative UTC timestamp from the
/// first `session_meta` event, which is always RFC 3339 with a `Z` suffix.
///
/// This is a small amount of extra I/O but only happens while looking for an
/// unresolved PID — once the provider caches the path, the scan stops.
fn scan_dir_for_candidates(
    dir: &Path,
    process_start_secs: u64,
    candidates: &mut Vec<(PathBuf, SystemTime)>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
            continue;
        }

        let session_start_secs = match read_session_start_secs(&path) {
            Some(ts) => ts as i64,
            None => continue,
        };

        let delta = session_start_secs - process_start_secs as i64;
        if delta.abs() > TIMESTAMP_TOLERANCE_SECS {
            continue;
        }

        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        candidates.push((path, mtime));
    }
}

/// Read the first line of `path` and extract the UTC session start time from
/// `session_meta.payload.timestamp`.
///
/// Returns `None` on any I/O error, unexpected event type, or parse failure —
/// the discovery scan treats these as "not a usable candidate" and moves on.
fn read_session_start_secs(path: &Path) -> Option<u64> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }

    let parsed: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if parsed.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let ts = parsed.get("payload")?.get("timestamp")?.as_str()?;
    parse_rfc3339_utc_secs(ts)
}

/// Parse an RFC 3339 timestamp like `2026-04-21T13:43:38.356Z` into Unix
/// epoch seconds. Only handles the `Z` suffix — that is what Codex writes.
fn parse_rfc3339_utc_secs(ts: &str) -> Option<u64> {
    // Expected: YYYY-MM-DDTHH:MM:SS[.fff]Z
    let (datetime, _frac_and_z) = ts.split_once('.').unwrap_or((ts.trim_end_matches('Z'), ""));
    let (date, time) = datetime.split_once('T')?;
    let mut date_parts = date.splitn(3, '-');
    let year: u32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.splitn(3, ':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    Some(ymd_hms_to_secs(year, month, day, hour, minute, second))
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
    // ---------------------------------------------------------------------------
    // parse_rollout_filename_secs
    // ---------------------------------------------------------------------------

    #[test]
    fn parse_filename_timestamp_valid() {
        // 2026-04-21T10:43:38 UTC = 1776174218 seconds
        let secs = parse_rollout_filename_secs(
            "rollout-2026-04-21T10-43-38-019db048-405b-73f3-8523-bf79aca97ace.jsonl",
        );
        assert!(secs.is_some(), "should parse a valid filename");
        let s = secs.unwrap();
        // Verify the year encodes correctly — result should be in 2026 (after 1 Jan 2026)
        assert!(s > 1_767_225_600, "should be after 2026-01-01");
        assert!(s < 1_798_761_600, "should be before 2027-01-01");
    }

    #[test]
    fn parse_filename_timestamp_epoch() {
        // 1970-01-01T00-00-00
        let secs = parse_rollout_filename_secs(
            "rollout-1970-01-01T00-00-00-00000000-0000-0000-0000.jsonl",
        );
        assert_eq!(secs, Some(0));
    }

    #[test]
    fn parse_filename_timestamp_invalid() {
        assert!(parse_rollout_filename_secs("not-a-rollout.jsonl").is_none());
        assert!(parse_rollout_filename_secs("rollout-bad.jsonl").is_none());
    }

    // ---------------------------------------------------------------------------
    // secs_to_ymd / days_from_civil round-trip
    // ---------------------------------------------------------------------------

    #[test]
    fn ymd_roundtrip_known_date() {
        // 2026-04-21 = 20574 days since epoch
        let secs = ymd_hms_to_secs(2026, 4, 21, 0, 0, 0);
        let (y, m, d) = secs_to_ymd(secs);
        assert_eq!((y, m, d), (2026, 4, 21));
    }

    #[test]
    fn ymd_roundtrip_epoch() {
        let secs = ymd_hms_to_secs(1970, 1, 1, 0, 0, 0);
        assert_eq!(secs, 0);
        let (y, m, d) = secs_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    // ---------------------------------------------------------------------------
    // extract_codex_home_from_env_string
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_codex_home_from_env_string_present() {
        let s = "/usr/bin/codex arg1 CODEX_HOME=/custom/home TERM=xterm";
        let result = extract_codex_home_from_env_string(s);
        assert_eq!(result.as_deref(), Some("/custom/home"));
    }

    #[test]
    fn extract_codex_home_from_env_string_absent() {
        let s = "/usr/bin/codex arg1 HOME=/Users/me TERM=xterm";
        let result = extract_codex_home_from_env_string(s);
        assert!(result.is_none());
    }

    #[test]
    fn extract_codex_home_from_env_string_empty() {
        let result = extract_codex_home_from_env_string("");
        assert!(result.is_none());
    }

    // ---------------------------------------------------------------------------
    // find_rollout_file
    // ---------------------------------------------------------------------------

    /// Write a rollout file with a valid `session_meta` header whose UTC
    /// timestamp drives the correlation.
    fn write_rollout_with_session_meta(path: &Path, session_ts_rfc3339_utc: &str) {
        let line = serde_json::json!({
            "type": "session_meta",
            "timestamp": session_ts_rfc3339_utc,
            "payload": {
                "id": "019db048-405b-73f3-8523-bf79aca97ace",
                "timestamp": session_ts_rfc3339_utc,
                "cwd": "/tmp",
                "cli_version": "0.121.0",
                "source": "cli",
                "model_provider": "openai",
            }
        })
        .to_string();
        std::fs::write(path, format!("{line}\n")).unwrap();
    }

    #[test]
    fn find_rollout_file_matches_within_tolerance() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("sessions");

        // Create 2026/04/21/ directory structure.
        let day_dir = sessions_root.join("2026").join("04").join("21");
        std::fs::create_dir_all(&day_dir).unwrap();

        // Process start: 2026-04-21T10:43:38 UTC.
        let process_start = ymd_hms_to_secs(2026, 4, 21, 10, 43, 38);

        // Session timestamp 22 s after the process start — well within the
        // 90-second tolerance. Filename (which is in *local* time) does not
        // need to match UTC; correlation reads session_meta.timestamp.
        let file_path = day_dir.join("rollout-2026-04-21T07-44-00-abc.jsonl");
        write_rollout_with_session_meta(&file_path, "2026-04-21T10:44:00.000Z");

        let codex_home = dir.path();
        let result = find_rollout_file(codex_home, process_start);
        assert!(result.is_some(), "should find a file within 90s tolerance");
    }

    #[test]
    fn find_rollout_file_rejects_outside_tolerance() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("sessions");
        let day_dir = sessions_root.join("2026").join("04").join("21");
        std::fs::create_dir_all(&day_dir).unwrap();

        // Process start: 2026-04-21T10:43:38 UTC.
        let process_start = ymd_hms_to_secs(2026, 4, 21, 10, 43, 38);

        // Session timestamp is 200 seconds later — outside 90s tolerance.
        let file_path = day_dir.join("rollout-2026-04-21T07-47-00-abc.jsonl");
        write_rollout_with_session_meta(&file_path, "2026-04-21T10:47:00.000Z");

        let result = find_rollout_file(dir.path(), process_start);
        assert!(result.is_none(), "200s gap should exceed tolerance");
    }

    #[test]
    fn find_rollout_file_ignores_filename_local_timestamp() {
        // Regression test: on a non-UTC host, the filename timestamp reflects
        // local time (e.g. São Paulo, UTC-3). The correlation must still work,
        // because we read the UTC timestamp from session_meta.payload.timestamp
        // instead of parsing the filename.
        let dir = tempfile::tempdir().unwrap();
        let sessions_root = dir.path().join("sessions");
        let day_dir = sessions_root.join("2026").join("04").join("21");
        std::fs::create_dir_all(&day_dir).unwrap();

        // Process started at 2026-04-21T13:43:38 UTC. The filename says
        // "10-43-38" because the Codex host was 3 hours west of UTC.
        let process_start = ymd_hms_to_secs(2026, 4, 21, 13, 43, 38);
        let file_path = day_dir.join("rollout-2026-04-21T10-43-38-abc.jsonl");
        write_rollout_with_session_meta(&file_path, "2026-04-21T13:43:38.356Z");

        let result = find_rollout_file(dir.path(), process_start);
        assert!(
            result.is_some(),
            "UTC-anchored correlation must not be fooled by local-time filenames"
        );
    }

    #[test]
    fn find_rollout_file_returns_none_for_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let process_start = ymd_hms_to_secs(2026, 4, 21, 10, 43, 38);
        let result = find_rollout_file(dir.path(), process_start);
        assert!(result.is_none());
    }

    // ---------------------------------------------------------------------------
    // CodexHomeResolver
    // ---------------------------------------------------------------------------

    #[test]
    fn default_sessions_dir_not_exists_on_fake_home() {
        // Create a resolver that has no ~/.codex/sessions directory.
        let dir = tempfile::tempdir().unwrap();
        let resolver = CodexHomeResolver {
            cache: HashMap::new(),
            default_home: Some(dir.path().join(".codex")),
        };
        // The directory was not created, so it should not exist.
        assert!(!resolver.default_sessions_dir_exists());
    }

    #[test]
    fn default_sessions_dir_exists_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        let sessions = codex_home.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();

        let resolver = CodexHomeResolver {
            cache: HashMap::new(),
            default_home: Some(codex_home),
        };
        assert!(resolver.default_sessions_dir_exists());
    }
}
