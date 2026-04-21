/// Truncate `s` to at most `max_chars` Unicode scalar values, appending
/// `"..."` when truncation occurred.
///
/// Byte-indexed slicing (e.g. `&s[..n]`) panics when `n` falls inside a
/// multi-byte UTF-8 sequence. Transcript content often contains em dashes,
/// smart quotes, and other multi-byte characters, so anywhere we clip
/// user-facing text we must count by chars rather than bytes.
///
/// # Examples
///
/// ```
/// use agentop::ui::truncate_chars;
/// assert_eq!(truncate_chars("hello", 10), "hello");
/// assert_eq!(truncate_chars("hello world", 5), "hello...");
/// // Multi-byte safe: an em dash is three bytes, one char.
/// assert_eq!(truncate_chars("a—b—c", 3), "a—b...");
/// ```
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{head}...")
    }
}

/// Format a byte count as a human-readable string with one decimal place.
///
/// # Examples
///
/// ```
/// use agentop::ui::format_memory;
/// assert_eq!(format_memory(1_500), "1.5 KB");
/// assert_eq!(format_memory(2_097_152), "2.0 MB");
/// ```
pub fn format_memory(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    }
}

/// Format a duration in seconds as a compact string (no seconds shown for durations >= 1 hour).
///
/// Produces `"Xd Xh Xm"` when days or hours are non-zero, otherwise `"Xm Xs"`.
///
/// # Examples
///
/// ```
/// use agentop::ui::format_duration_compact;
/// assert_eq!(format_duration_compact(3661), "0d 1h 1m");
/// assert_eq!(format_duration_compact(90), "1m 30s");
/// ```
pub fn format_duration_compact(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let mins = (seconds % 3_600) / 60;
    let secs = seconds % 60;

    if days > 0 || hours > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else {
        format!("{}m {}s", mins, secs)
    }
}

/// Format a duration in seconds as a full string including days, hours, minutes, and seconds.
///
/// # Examples
///
/// ```
/// use agentop::ui::format_duration_full;
/// assert_eq!(format_duration_full(90061), "1d 1h 1m 1s");
/// ```
pub fn format_duration_full(seconds: u64) -> String {
    let d = seconds / 86_400;
    let h = (seconds % 86_400) / 3_600;
    let m = (seconds % 3_600) / 60;
    let s = seconds % 60;
    format!("{}d {}h {}m {}s", d, h, m, s)
}
