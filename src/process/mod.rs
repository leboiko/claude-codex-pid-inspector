/// Process data snapshot and helper types.
pub mod info;

/// Predicates for identifying Claude Code and Codex CLI processes.
pub mod filter;

/// Process tree construction and flat-list projection.
pub mod tree;

/// Live process scanning via `sysinfo`.
pub mod scanner;

pub use filter::ProcessKind;
pub use info::ProcessInfo;
pub use scanner::ProcessScanner;
pub use tree::{
    build_forest, collect_expansion, flatten_visible, preserve_expansion, toggle_expand, FlatEntry,
    ProcessNode,
};
