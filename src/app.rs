use std::collections::{HashMap, HashSet, VecDeque};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::action::Action;
use crate::process::{
    build_forest, collect_expansion, flatten_visible, preserve_expansion, toggle_expand, FlatEntry,
    ProcessInfo, ProcessNode, SystemStats,
};

/// Maximum number of historical CPU/memory samples retained per process.
const HISTORY_LEN: usize = 30;

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
        Self::Pid, Self::Name, Self::Cpu, Self::Memory, Self::Status, Self::Uptime,
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

/// Tracks which top-level panel is currently receiving input and being rendered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ActiveView {
    /// The scrollable process tree list.
    #[default]
    Tree,
    /// The drill-down detail panel for a single selected process.
    Detail,
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
}

impl App {
    /// Create a new [`App`] with sensible defaults and row 0 pre-selected.
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        // Pre-select the first row so the cursor is always visible from the start.
        table_state.select(Some(0));
        Self {
            table_state,
            ..Default::default()
        }
    }

    /// Dispatch an [`Action`] produced by the event loop, mutating state accordingly.
    pub fn handle_action(&mut self, action: Action) {
        // Clear the kill result message on any action that isn't part of the kill flow.
        if !matches!(action, Action::KillRequest | Action::ConfirmKill | Action::CancelKill) {
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
    }

    /// Sort the forest in place, then flatten into `flat_list`.
    ///
    /// Sorting is done on the tree before flattening so sibling order at every
    /// depth level is correct and parent-child grouping is never violated.
    fn sort_flat_list(&mut self) {
        sort_forest(&mut self.forest, self.sort_column, self.sort_direction);
        self.flat_list = flatten_visible(&self.forest);
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
    ) -> Option<Action> {
        // Ctrl+C is a universal quit regardless of view or mode.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
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
                _ => None,
            },
            ActiveView::Detail => match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Esc => Some(Action::BackToTree),
                KeyCode::Char('x') => Some(Action::KillRequest),
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
/// # Arguments
///
/// * `nodes`     - Mutable slice of sibling nodes to sort at this level.
/// * `column`    - The column to compare on.
/// * `direction` - Ascending or descending order.
fn sort_forest(nodes: &mut [ProcessNode], column: SortColumn, direction: SortDirection) {
    nodes.sort_by(|a, b| {
        let cmp = compare_by_column(&a.info, &b.info, column);
        match direction {
            SortDirection::Ascending => cmp,
            SortDirection::Descending => cmp.reverse(),
        }
    });
    // Recurse so every sibling group at every depth is sorted.
    for node in nodes.iter_mut() {
        sort_forest(&mut node.children, column, direction);
    }
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
