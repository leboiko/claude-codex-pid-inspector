use std::collections::{HashMap, HashSet, VecDeque};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::action::Action;
use crate::config::Config;
use crate::process::{
    build_forest, collect_expansion, flatten_visible, preserve_expansion, process_kind,
    toggle_expand, FlatEntry, ProcessInfo, ProcessKind, ProcessNode, SubtreeStats, SystemStats,
};
use crate::ui::styles::{GraphStyle, Palette, Theme};

/// Maximum number of historical CPU/memory samples retained per process.
///
/// At the default 2-second tick rate this is ~10 minutes of history,
/// enough to fill the sparkline chart on any realistic terminal width.
const HISTORY_LEN: usize = 300;

/// Columns that support sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortColumn {
    #[default]
    Pid,
    Name,
    Cpu,
    Memory,
    Status,
    Uptime,
}

impl SortColumn {
    const ALL: [SortColumn; 6] = [
        Self::Pid,
        Self::Name,
        Self::Cpu,
        Self::Memory,
        Self::Status,
        Self::Uptime,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL
            .iter()
            .position(|&c| c == self)
            .expect("SortColumn variant missing from ALL array");
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL
            .iter()
            .position(|&c| c == self)
            .expect("SortColumn variant missing from ALL array");
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    Ascending,
    #[default]
    Descending,
}

impl SortDirection {
    pub fn toggle(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// Aggregated counts and resource usage across all detected agent processes.
///
/// Computed after every process refresh via [`App::compute_agent_summary`] and
/// displayed in the status bar to give an at-a-glance overview of active agents.
#[derive(Debug, Clone, Default)]
pub struct AgentSummary {
    /// Number of Claude Code root processes currently running.
    pub claude_count: usize,
    /// Number of Codex CLI root processes currently running.
    pub codex_count: usize,
    /// Total CPU usage (%) across all agent subtrees.
    pub total_cpu: f32,
    /// Total resident memory (bytes) across all agent subtrees.
    pub total_memory: u64,
}

/// Tracks which top-level panel is currently receiving input and being rendered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ActiveView {
    /// The scrollable process tree list.
    #[default]
    Tree,
    /// The drill-down detail panel for a single selected process.
    Detail,
}

/// Transient state for the settings popup.
///
/// The popup exposes two categories of options — `Graph Style` and `Theme` —
/// as a single flat list so Up/Down navigation is trivial. [`Self::SECTIONS`]
/// describes the list layout used by both the handler and the renderer.
#[derive(Debug, Clone, Default)]
pub struct ConfigPopupState {
    /// Currently highlighted row in the flat option list.
    pub cursor: usize,
}

impl ConfigPopupState {
    /// Ordered sections and the option count for each.
    ///
    /// `(section label, option count)`. The flat cursor index maps into this
    /// layout so row 0 is the first option of the first section.
    pub const SECTIONS: &'static [(&'static str, usize)] = &[
        ("Graph Style", GraphStyle::ALL.len()),
        ("Theme", Theme::ALL.len()),
    ];

    /// Total number of selectable rows across all sections.
    pub fn total_rows() -> usize {
        Self::SECTIONS.iter().map(|(_, n)| n).sum()
    }

    /// Move the cursor up one row, wrapping at the top.
    pub fn move_up(&mut self) {
        let total = Self::total_rows();
        self.cursor = (self.cursor + total - 1) % total;
    }

    /// Move the cursor down one row, wrapping at the bottom.
    pub fn move_down(&mut self) {
        let total = Self::total_rows();
        self.cursor = (self.cursor + 1) % total;
    }
}

/// Central application state. All mutations flow through [`App::handle_action`]
/// or [`App::update_processes`], keeping the state machine easy to reason about.
#[derive(Debug, Default)]
pub struct App {
    /// Set to `true` when the event loop should exit.
    pub should_quit: bool,
    /// Which panel currently owns keyboard focus.
    pub active_view: ActiveView,
    /// Live process tree, kept in sync with each refresh cycle.
    pub forest: Vec<ProcessNode>,
    /// Ordered, flattened projection of the visible forest rows.
    pub flat_list: Vec<FlatEntry>,
    /// Drives the ratatui `Table` cursor — holds the currently highlighted row index.
    pub table_state: TableState,
    /// Process snapshot shown in the detail panel; `None` until a row is selected.
    pub selected_detail: Option<ProcessInfo>,
    /// Subtree statistics for the selected process; `None` until a row is selected.
    pub selected_detail_subtree: Option<SubtreeStats>,
    /// Rolling CPU-usage history per PID (percentage, up to [`HISTORY_LEN`] samples).
    pub cpu_history: HashMap<u32, VecDeque<f32>>,
    /// Rolling resident-memory history per PID (bytes, up to [`HISTORY_LEN`] samples).
    pub mem_history: HashMap<u32, VecDeque<u64>>,
    /// Active sort column.
    pub sort_column: SortColumn,
    /// Active sort direction.
    pub sort_direction: SortDirection,
    /// PID pending kill confirmation.
    pub confirm_kill_pid: Option<u32>,
    /// Result message from the last kill attempt.
    pub kill_result: Option<String>,
    /// Latest system-wide resource snapshot.
    pub system_stats: SystemStats,
    /// Active visual theme. Changing this regenerates `palette`.
    pub theme: Theme,
    /// Cached style palette derived from `theme`, passed to every renderer.
    pub palette: Palette,
    /// Active graph style for the detail view (dots vs bars).
    pub graph_style: GraphStyle,
    /// Settings popup state; `None` when the popup is closed.
    pub config_popup: Option<ConfigPopupState>,
    /// Latest aggregated summary across all detected agent processes.
    pub agent_summary: AgentSummary,
}

impl App {
    /// Create a new [`App`] with sensible defaults and row 0 pre-selected.
    ///
    /// Persisted settings are loaded from
    /// `$XDG_CONFIG_HOME/agentop/config.toml` (or the platform equivalent)
    /// and applied to `theme` / `graph_style`. If no config file exists the
    /// defaults defined by the enum `Default` impls are used.
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        // Pre-select the first row so the cursor is always visible from the start.
        table_state.select(Some(0));

        let config = Config::load();
        let palette = Palette::from_theme(config.theme);

        Self {
            table_state,
            theme: config.theme,
            graph_style: config.graph_style,
            palette,
            ..Default::default()
        }
    }

    /// Write the currently-applied settings to disk. Called whenever the
    /// user changes a setting via the config popup.
    fn persist_config(&self) {
        Config {
            theme: self.theme,
            graph_style: self.graph_style,
        }
        .save();
    }

    /// Dispatch an [`Action`] produced by the event loop, mutating state accordingly.
    pub fn handle_action(&mut self, action: Action) {
        // Clear the kill result message on any action that isn't part of the kill flow.
        if !matches!(
            action,
            Action::KillRequest | Action::ConfirmKill | Action::CancelKill
        ) {
            self.kill_result = None;
        }

        match action {
            Action::Quit => self.should_quit = true,
            Action::MoveUp => self.move_selection(-1),
            Action::MoveDown => self.move_selection(1),
            Action::ToggleExpand => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(entry) = self.flat_list.get(idx) {
                        let pid = entry.info.pid;
                        toggle_expand(&mut self.forest, pid);
                        self.rebuild_flat_list();
                    }
                }
            }
            Action::SelectProcess => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(entry) = self.flat_list.get(idx) {
                        self.selected_detail = Some(entry.info.clone());
                        self.selected_detail_subtree = Some(entry.subtree_stats);
                        self.active_view = ActiveView::Detail;
                    }
                }
            }
            Action::BackToTree => {
                self.active_view = ActiveView::Tree;
            }
            Action::SortNext => {
                self.sort_column = self.sort_column.next();
                self.rebuild_flat_list();
            }
            Action::SortPrev => {
                self.sort_column = self.sort_column.prev();
                self.rebuild_flat_list();
            }
            Action::SortToggleDirection => {
                self.sort_direction = self.sort_direction.toggle();
                self.rebuild_flat_list();
            }
            Action::KillRequest => {
                let pid = self.selected_pid();
                if pid.is_some() {
                    self.confirm_kill_pid = pid;
                    self.kill_result = None;
                }
            }
            Action::ConfirmKill => {
                if let Some(pid) = self.confirm_kill_pid.take() {
                    self.kill_result = Some(kill_process(pid));
                }
            }
            Action::CancelKill => {
                self.confirm_kill_pid = None;
            }
            Action::ToggleConfig => {
                if self.config_popup.is_some() {
                    self.config_popup = None;
                } else {
                    self.config_popup = Some(ConfigPopupState::default());
                }
            }
            Action::ConfigUp => {
                if let Some(popup) = self.config_popup.as_mut() {
                    popup.move_up();
                }
            }
            Action::ConfigDown => {
                if let Some(popup) = self.config_popup.as_mut() {
                    popup.move_down();
                }
            }
            Action::ConfigSelect => {
                if let Some(popup) = self.config_popup.as_ref() {
                    self.apply_config_selection(popup.cursor);
                }
            }
            Action::CloseConfig => {
                self.config_popup = None;
            }
        }
    }

    /// Apply the option at the given flat cursor index to the live app state.
    ///
    /// The layout is driven by [`ConfigPopupState::SECTIONS`] so adding a new
    /// section or option does not require touching this function's arm order.
    ///
    /// Writes the updated settings to disk so they persist across restarts.
    fn apply_config_selection(&mut self, cursor: usize) {
        let mut offset = 0;
        // Section 0: graph style.
        let graph_count = GraphStyle::ALL.len();
        if cursor < offset + graph_count {
            self.graph_style = GraphStyle::ALL[cursor - offset];
            self.persist_config();
            return;
        }
        offset += graph_count;

        // Section 1: theme. Regenerate the palette so render functions
        // immediately pick up the new colors.
        let theme_count = Theme::ALL.len();
        if cursor < offset + theme_count {
            let theme = Theme::ALL[cursor - offset];
            self.theme = theme;
            self.palette = Palette::from_theme(theme);
            self.persist_config();
        }
    }

    /// Move the highlighted row by `delta` rows, clamping at the list boundaries.
    ///
    /// # Arguments
    ///
    /// * `delta` - Positive values move down; negative values move up.
    fn move_selection(&mut self, delta: i32) {
        let len = self.flat_list.len();
        if len == 0 {
            return;
        }
        // current defaults to 0 when nothing is selected yet.
        let current = self.table_state.selected().unwrap_or(0) as i32;
        // Clamp to [0, len - 1] to prevent out-of-bounds selection.
        let next = (current + delta).clamp(0, (len as i32) - 1) as usize;
        self.table_state.select(Some(next));
    }

    /// Ingest a fresh process snapshot, preserving expansion state and updating histories.
    ///
    /// This is the primary entry point called by the background scanner on each tick.
    ///
    /// # Arguments
    ///
    /// * `processes` - Complete flat list of process snapshots from the current refresh.
    pub fn update_processes(&mut self, processes: Vec<ProcessInfo>, stats: SystemStats) {
        self.system_stats = stats;
        // Snapshot expansion state before rebuilding so the user's open/close choices survive.
        let old_expansion = collect_expansion(&self.forest);

        self.update_history(&processes);

        // Prune history for processes that no longer exist, preventing unbounded growth.
        let live_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
        self.cpu_history.retain(|pid, _| live_pids.contains(pid));
        self.mem_history.retain(|pid, _| live_pids.contains(pid));

        self.forest = build_forest(&processes);
        preserve_expansion(&mut self.forest, &old_expansion);

        // Keep the detail view in sync with live data.
        if let Some(ref mut detail) = self.selected_detail {
            if let Some(updated) = processes.iter().find(|p| p.pid == detail.pid) {
                *detail = updated.clone();
            }
        }

        self.rebuild_flat_list();

        // Refresh the subtree stats for the selected process from the newly
        // flattened list, so the detail view always shows current aggregate data.
        if let Some(ref detail) = self.selected_detail {
            let pid = detail.pid;
            self.selected_detail_subtree = self
                .flat_list
                .iter()
                .find(|e| e.info.pid == pid)
                .map(|e| e.subtree_stats);
        }

        self.agent_summary = self.compute_agent_summary();
    }

    /// Sort the forest in place, then flatten into `flat_list`.
    ///
    /// Sorting is done on the tree before flattening so sibling order at every
    /// depth level is correct and parent-child grouping is never violated.
    /// Root nodes are sorted by aggregate (subtree) stats; child nodes by self stats.
    fn sort_flat_list(&mut self) {
        sort_forest(
            &mut self.forest,
            self.sort_column,
            self.sort_direction,
            true,
        );
        self.flat_list = flatten_visible(&self.forest);
    }

    /// Compute the [`AgentSummary`] from the current forest.
    ///
    /// Iterates over root nodes only: each root's `subtree_stats` already
    /// carries the recursive aggregate, so no additional traversal is needed.
    pub fn compute_agent_summary(&self) -> AgentSummary {
        let mut summary = AgentSummary::default();
        for root in &self.forest {
            match process_kind(&root.info) {
                Some(ProcessKind::Claude) => summary.claude_count += 1,
                Some(ProcessKind::Codex) => summary.codex_count += 1,
                None => {}
            }
            summary.total_cpu += root.subtree_stats.total_cpu;
            summary.total_memory += root.subtree_stats.total_memory;
        }
        summary
    }

    /// Rebuild and sort `flat_list`, then clamp the selection cursor.
    ///
    /// Call this whenever the forest structure or sort parameters change.
    fn rebuild_flat_list(&mut self) {
        self.sort_flat_list();
        self.clamp_selection();
    }

    /// Return the PID of the currently focused process, if any.
    fn selected_pid(&self) -> Option<u32> {
        match self.active_view {
            ActiveView::Tree => {
                let idx = self.table_state.selected()?;
                Some(self.flat_list.get(idx)?.info.pid)
            }
            ActiveView::Detail => self.selected_detail.as_ref().map(|d| d.pid),
        }
    }

    /// Clamp the selected row index to valid bounds.
    fn clamp_selection(&mut self) {
        let len = self.flat_list.len();
        if len == 0 {
            self.table_state.select(None);
            return;
        }
        let clamped = self.table_state.selected().unwrap_or(0).min(len - 1);
        self.table_state.select(Some(clamped));
    }

    /// Push the latest CPU and memory readings into the per-PID ring buffers.
    ///
    /// Called before the forest is rebuilt so it operates on the raw flat list,
    /// reaching every process regardless of tree depth.
    ///
    /// # Arguments
    ///
    /// * `processes` - The same flat snapshot slice passed to [`update_processes`].
    fn update_history(&mut self, processes: &[ProcessInfo]) {
        for proc in processes {
            // VecDeque as a fixed-size ring buffer: push to back, pop from front.
            let cpu_buf = self.cpu_history.entry(proc.pid).or_default();
            if cpu_buf.len() == HISTORY_LEN {
                cpu_buf.pop_front();
            }
            cpu_buf.push_back(proc.cpu_usage);

            let mem_buf = self.mem_history.entry(proc.pid).or_default();
            if mem_buf.len() == HISTORY_LEN {
                mem_buf.pop_front();
            }
            mem_buf.push_back(proc.memory_bytes);
        }
    }

    /// Translate a raw terminal key event into an [`Action`], if one is bound.
    ///
    /// Returns `None` for unbound keys so the caller can ignore them without matching
    /// exhaustively on every possible [`KeyCode`].
    ///
    /// # Arguments
    ///
    /// * `key`         - The raw key event from crossterm.
    /// * `active_view` - The panel currently in focus; some bindings are view-specific.
    pub fn map_key_to_action(
        key: KeyEvent,
        active_view: &ActiveView,
        confirming_kill: bool,
        config_open: bool,
    ) -> Option<Action> {
        // Ctrl+C is a universal quit regardless of view or mode.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }

        // Config popup captures all input when open. 'c' toggles it closed;
        // Esc also closes without applying a new selection.
        if config_open {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') => Some(Action::ConfigUp),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::ConfigDown),
                KeyCode::Enter => Some(Action::ConfigSelect),
                KeyCode::Esc | KeyCode::Char('c') => Some(Action::CloseConfig),
                KeyCode::Char('q') => Some(Action::Quit),
                _ => None,
            };
        }

        // When a kill confirmation is pending, only y/n/Esc are accepted.
        if confirming_kill {
            return match key.code {
                KeyCode::Char('y') => Some(Action::ConfirmKill),
                KeyCode::Char('n') | KeyCode::Esc => Some(Action::CancelKill),
                _ => None,
            };
        }

        match active_view {
            ActiveView::Tree => match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Up | KeyCode::Char('k') => Some(Action::MoveUp),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::MoveDown),
                KeyCode::Char(' ') => Some(Action::ToggleExpand),
                KeyCode::Enter => Some(Action::SelectProcess),
                KeyCode::Tab => Some(Action::SortNext),
                KeyCode::BackTab => Some(Action::SortPrev),
                KeyCode::Char('s') => Some(Action::SortToggleDirection),
                KeyCode::Char('x') => Some(Action::KillRequest),
                KeyCode::Char('c') => Some(Action::ToggleConfig),
                _ => None,
            },
            ActiveView::Detail => match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Esc => Some(Action::BackToTree),
                KeyCode::Char('x') => Some(Action::KillRequest),
                KeyCode::Char('c') => Some(Action::ToggleConfig),
                _ => None,
            },
        }
    }
}

/// Attempt to kill a process by PID using SIGTERM.
///
/// Uses `libc::kill` directly instead of sysinfo, which requires a
/// fully-refreshed `System` instance just to send a signal.
fn kill_process(pid: u32) -> String {
    let pid_i32 = pid as i32;
    // SAFETY: kill(2) with SIGTERM is a standard POSIX syscall.
    let result = unsafe { libc::kill(pid_i32, libc::SIGTERM) };
    if result == 0 {
        return format!("Sent SIGTERM to PID {}", pid);
    }

    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ESRCH) => format!("PID {} not found", pid),
        Some(libc::EPERM) => format!("Permission denied for PID {}", pid),
        _ => format!("Failed to kill PID {}: {}", pid, err),
    }
}

/// Sort process nodes recursively: siblings at each level are sorted,
/// preserving the parent-child tree structure.
///
/// When `use_aggregate` is `true` the root-level siblings are sorted by their
/// subtree aggregate values for CPU and Memory columns, giving a more useful
/// ordering (e.g. a claude instance using 400 MB of child memory ranks higher
/// than one using 10 MB). Children are always sorted by their own `info` values.
///
/// # Arguments
///
/// * `nodes`         - Mutable slice of sibling nodes to sort at this level.
/// * `column`        - The column to compare on.
/// * `direction`     - Ascending or descending order.
/// * `use_aggregate` - When `true`, sort this level by subtree stats for
///   CPU/Memory columns. Set to `false` for recursive calls.
fn sort_forest(
    nodes: &mut [ProcessNode],
    column: SortColumn,
    direction: SortDirection,
    use_aggregate: bool,
) {
    nodes.sort_by(|a, b| {
        let cmp = compare_nodes(a, b, column, use_aggregate);
        match direction {
            SortDirection::Ascending => cmp,
            SortDirection::Descending => cmp.reverse(),
        }
    });
    // Children are always sorted by their own stats (use_aggregate = false).
    for node in nodes.iter_mut() {
        sort_forest(&mut node.children, column, direction, false);
    }
}

/// Compare two [`ProcessNode`] values by the given sort column.
///
/// When `use_aggregate` is `true` and the column is `Cpu` or `Memory`, the
/// comparison uses `subtree_stats` totals so root nodes are ranked by their
/// full resource footprint rather than just the top-level process.
fn compare_nodes(
    a: &ProcessNode,
    b: &ProcessNode,
    column: SortColumn,
    use_aggregate: bool,
) -> std::cmp::Ordering {
    if use_aggregate {
        match column {
            SortColumn::Cpu => {
                return a
                    .subtree_stats
                    .total_cpu
                    .partial_cmp(&b.subtree_stats.total_cpu)
                    .unwrap_or(std::cmp::Ordering::Equal);
            }
            SortColumn::Memory => {
                return a
                    .subtree_stats
                    .total_memory
                    .cmp(&b.subtree_stats.total_memory);
            }
            _ => {}
        }
    }
    compare_by_column(&a.info, &b.info, column)
}

/// Compare two [`ProcessInfo`] values by the given sort column.
fn compare_by_column(a: &ProcessInfo, b: &ProcessInfo, column: SortColumn) -> std::cmp::Ordering {
    match column {
        SortColumn::Pid => a.pid.cmp(&b.pid),
        SortColumn::Name => a.name.cmp(&b.name),
        SortColumn::Cpu => a
            .cpu_usage
            .partial_cmp(&b.cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal),
        SortColumn::Memory => a.memory_bytes.cmp(&b.memory_bytes),
        SortColumn::Status => a.status.cmp(&b.status),
        SortColumn::Uptime => a.run_time.cmp(&b.run_time),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{build_forest, ProcessInfo};

    fn make_proc(pid: u32, parent: Option<u32>, name: &str, cpu: f32, mem: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: parent,
            name: name.to_string(),
            cmd: vec![name.to_string()],
            exe_path: None,
            cwd: None,
            cpu_usage: cpu,
            memory_bytes: mem,
            status: "Run".to_string(),
            environ_count: 0,
            start_time: 0,
            run_time: 0,
        }
    }

    #[test]
    fn test_agent_summary_empty() {
        // An empty forest should produce a zeroed AgentSummary.
        let app = App {
            forest: build_forest(&[]),
            ..Default::default()
        };
        let summary = app.compute_agent_summary();
        assert_eq!(summary.claude_count, 0);
        assert_eq!(summary.codex_count, 0);
        assert_eq!(summary.total_memory, 0);
        assert!((summary.total_cpu - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_agent_summary_mixed() {
        // 2 claude roots + 1 codex root, each with one child.
        // subtree_stats should be aggregated (self + child).
        let procs = vec![
            make_proc(1, None, "claude", 1.0, 100),
            make_proc(2, Some(1), "node", 1.0, 100), // child of claude 1
            make_proc(3, None, "claude", 2.0, 200),
            make_proc(4, Some(3), "node", 2.0, 200), // child of claude 3
            make_proc(5, None, "codex", 3.0, 300),
            make_proc(6, Some(5), "node", 3.0, 300), // child of codex
        ];
        let app = App {
            forest: build_forest(&procs),
            ..Default::default()
        };
        let summary = app.compute_agent_summary();
        assert_eq!(summary.claude_count, 2);
        assert_eq!(summary.codex_count, 1);
        // Each root's subtree_stats covers self + one child, so:
        // claude1: cpu=2.0, mem=200; claude3: cpu=4.0, mem=400; codex5: cpu=6.0, mem=600
        assert!(
            (summary.total_cpu - 12.0).abs() < 1e-3,
            "total cpu: {}",
            summary.total_cpu
        );
        assert_eq!(summary.total_memory, 1200);
    }
}
