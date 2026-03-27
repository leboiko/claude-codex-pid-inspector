/// Format a byte count as a human-readable string with one decimal place.
///
/// # Examples
///
/// ```
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
/// assert_eq!(format_duration_compact(3661), "1h 1m"); // wait, days=0, hours=1 => "0d 1h 1m"
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
/// assert_eq!(format_duration_full(90061), "1d 1h 1m 1s");
/// ```
pub fn format_duration_full(seconds: u64) -> String {
    let d = seconds / 86_400;
    let h = (seconds % 86_400) / 3_600;
    let m = (seconds % 3_600) / 60;
    let s = seconds % 60;
    format!("{}d {}h {}m {}s", d, h, m, s)
}
