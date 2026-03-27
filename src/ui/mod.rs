mod detail_view;
mod footer;
mod format;
mod popup;
mod styles;
mod tree_view;

pub use detail_view::render_detail_view;
pub use footer::render_footer;
#[allow(unused_imports)]
pub use format::{format_duration_compact, format_duration_full, format_memory};
pub use popup::{render_kill_confirm, render_kill_result};
pub use tree_view::render_tree_view;
