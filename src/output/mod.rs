//! Non-interactive output modes: `--list` (plain-text table) and `--json` (JSON snapshot).

pub mod json;
pub mod list;

pub use json::{build_snapshot, build_snapshot_with_telemetry, to_compact_json, to_pretty_json};
pub use list::print_table;
