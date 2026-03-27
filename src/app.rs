use std::collections::{HashMap, VecDeque};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::action::Action;
use crate::process::{
    build_forest, collect_expansion, flatten_visible, preserve_expansion, toggle_expand, FlatEntry,
    ProcessInfo, ProcessNode,
};

/// Maximum number of historical CPU/memory samples retained per process.
const HISTORY_LEN: usize = 30;

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
    pub fn update_processes(&mut self, processes: Vec<ProcessInfo>) {
        // Snapshot expansion state before rebuilding so the user's open/close choices survive.
        let old_expansion = collect_expansion(&self.forest);

        self.update_history(&processes);

        self.forest = build_forest(&processes);
        preserve_expansion(&mut self.forest, &old_expansion);

        self.rebuild_flat_list();
    }

    /// Flatten the current forest into `flat_list` and clamp the selection index.
    fn rebuild_flat_list(&mut self) {
        self.flat_list = flatten_visible(&self.forest);

        let len = self.flat_list.len();
        if len == 0 {
            self.table_state.select(None);
            return;
        }
        // Clamp: if a previously selected row index is now past the end (e.g. a process
        // disappeared), pin it to the last available row.
        let clamped = self
            .table_state
            .selected()
            .unwrap_or(0)
            .min(len - 1);
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
    pub fn map_key_to_action(key: KeyEvent, active_view: &ActiveView) -> Option<Action> {
        // Ctrl+C is a universal quit regardless of view or mode.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }

        match active_view {
            ActiveView::Tree => match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Up | KeyCode::Char('k') => Some(Action::MoveUp),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::MoveDown),
                KeyCode::Char(' ') => Some(Action::ToggleExpand),
                KeyCode::Enter => Some(Action::SelectProcess),
                // 'r' reserved for manual refresh — no-op until wired up.
                _ => None,
            },
            ActiveView::Detail => match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Esc => Some(Action::BackToTree),
                _ => None,
            },
        }
    }
}
