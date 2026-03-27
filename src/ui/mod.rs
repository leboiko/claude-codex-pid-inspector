mod detail_view;
mod footer;
mod format;
mod styles;
mod tree_view;

pub use detail_view::render_detail_view;
pub use footer::render_footer;
// Re-exported for callers outside this module; allow the lint since the binary
// itself does not call these directly (they are used by submodules internally).
#[allow(unused_imports)]
pub use format::{format_duration_compact, format_duration_full, format_memory};
pub use tree_view::render_tree_view;
