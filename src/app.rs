use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::action::Action;
use crate::config::Config;
use crate::process::{
    build_forest, collect_expansion, flatten_visible, preserve_expansion, process_kind,
    toggle_expand, ActivityState, FlatEntry, FlatEntryKind, ProcessInfo, ProcessKind, ProcessNode,
    SubtreeStats, SystemStats,
};
use crate::telemetry::{AgentStatus, TelemetryMap};
use crate::ui::styles::{GraphStyle, Palette, Theme};

/// How long a transient status message (e.g. terminal jump result) stays visible.
const STATUS_MESSAGE_TTL_SECS: u64 = 3;

/// Maximum number of historical CPU/memory samples retained per process.
///
/// At the default 2-second tick rate this is ~10 minutes of history,
/// enough to fill the sparkline chart on any realistic terminal width.
const HISTORY_LEN: usize = 300;

/// CPU usage percentage below which a root process is considered idle.
pub const IDLE_CPU_THRESHOLD: f32 = 0.5;

/// Minimum number of CPU samples required before classifying a process
/// as idle or active. Fewer samples yield [`ActivityState::Unknown`].
pub const IDLE_SAMPLE_WINDOW: usize = 5;

/// The curated focus filters selectable with the `F` key.
///
/// Cycles in the order defined by the `ALL` constant. The active filter is
/// applied before project grouping in the flat-list rebuild pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusFilter {
    /// No filter — all processes are shown (default).
    #[default]
    All,
    /// Processes that need attention: status is [`AgentStatus::NeedsInput`]
    /// or context fraction is at or above 80 %.
    Attention,
    /// Processes with smoothed CPU usage >= 30 %.
    HighCpu,
    /// Processes with context fraction >= 80 %.
    HighContext,
    /// Processes that started within the last 10 minutes (run_time <= 600 s).
    Recent,
}

impl FocusFilter {
    /// All focus-filter variants in the cycle order.
    const ALL: [FocusFilter; 5] = [
        Self::All,
        Self::Attention,
        Self::HighCpu,
        Self::HighContext,
        Self::Recent,
    ];

    /// Advance to the next filter in the cycle, wrapping at the end.
    pub fn next(self) -> Self {
        let idx = Self::ALL
            .iter()
            .position(|&f| f == self)
            .expect("FocusFilter variant missing from ALL array");
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    /// Short label rendered in the status-bar filter pill.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Attention => "attention",
            Self::HighCpu => "high-cpu",
            Self::HighContext => "high-context",
            Self::Recent => "recent",
        }
    }
}

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

/// Context passed to [`App::map_key_to_action`] describing the current UI mode.
///
/// Bundling these boolean flags into a struct avoids a long parameter list and
/// makes it straightforward to add new mode flags in the future without breaking
/// all call sites.
pub struct KeyContext<'a> {
    /// The panel that currently owns keyboard focus.
    pub active_view: &'a ActiveView,
    /// Whether a kill-confirmation popup is currently open.
    pub confirming_kill: bool,
    /// Whether the config popup is currently open.
    pub config_open: bool,
    /// Whether the search/filter bar is currently active.
    pub filter_active: bool,
    /// Whether the help overlay is currently open.
    pub help_open: bool,
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
    /// Rolling CPU-usage history per PID (percentage, up to `HISTORY_LEN` samples).
    pub cpu_history: HashMap<u32, VecDeque<f32>>,
    /// Rolling resident-memory history per PID (bytes, up to `HISTORY_LEN` samples).
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
    /// Per-root-PID activity classification (`Active` / `Idle` / `Unknown`).
    pub activity_state: HashMap<u32, ActivityState>,
    /// Per-root-PID aggregate CPU history used for idle/active classification.
    pub aggregate_cpu_history: HashMap<u32, VecDeque<f32>>,
    /// Per-root-PID timestamp of when the current activity state began.
    ///
    /// Populated by [`App::update_activity_states`] whenever the state transitions.
    /// Used by the renderer to compute time-in-state duration for color escalation.
    pub activity_state_since: HashMap<u32, Instant>,
    /// Whether the search/filter bar is currently accepting input.
    pub filter_active: bool,
    /// Current filter query text.
    pub filter_text: String,
    /// Per-PID telemetry enriched by the telemetry pipeline on each tick.
    ///
    /// Keyed by PID; entries are pruned alongside `cpu_history` when a process
    /// exits. Phase 0 always delivers an empty map (only the [`crate::telemetry::NoopProvider`]
    /// is registered); Phase 2 populates real values for Claude sessions.
    pub telemetry: TelemetryMap,

    /// When `true`, the process tree shows the agent telemetry column set instead
    /// of the default CPU/memory columns. Toggled with the `t` key.
    pub telemetry_view: bool,

    /// Per-PID cost burn tracking: `(Instant, cost_usd_at_sample)`.
    ///
    /// Used to detect a cost increase of > $0.10 in the last 30 seconds and
    /// append a burn indicator (`↑`) to the cost cell in the agent view.
    pub cost_burn_history: HashMap<u32, (Instant, f64)>,

    /// Currently active curated focus filter. Cycled with `F`.
    pub focus_filter: FocusFilter,

    /// Whether project grouping (group rows by cwd) is enabled. Toggled with `g`.
    pub project_grouping: bool,

    /// Whether the help overlay is currently visible.
    pub help_open: bool,

    /// Transient status message (e.g. "Jumped to tmux session:1.0") and when it
    /// was set. Cleared automatically after 3 seconds.
    pub status_message: Option<(String, Instant)>,

    /// Collapse state per cwd group key when project grouping is active.
    ///
    /// `false` means collapsed (children hidden). Absent entries default to expanded.
    pub group_collapsed: HashMap<String, bool>,
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

    /// Expire the transient status message if it has been visible long enough.
    ///
    /// Call this on every render tick so messages disappear automatically.
    pub fn tick_status_message(&mut self) {
        if let Some((_, set_at)) = self.status_message {
            if set_at.elapsed().as_secs() >= STATUS_MESSAGE_TTL_SECS {
                self.status_message = None;
            }
        }
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
                    if let Some(entry) = self.flat_list.get(idx).cloned() {
                        match &entry.row_kind {
                            FlatEntryKind::GroupHeader {
                                cwd_key, expanded, ..
                            } => {
                                // Toggle collapse state for this cwd group.
                                let key = cwd_key.clone();
                                self.group_collapsed.insert(key, !expanded);
                                self.rebuild_flat_list();
                            }
                            FlatEntryKind::Process => {
                                let pid = entry.info.pid;
                                toggle_expand(&mut self.forest, pid);
                                self.rebuild_flat_list();
                            }
                        }
                    }
                }
            }
            Action::SelectProcess => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(entry) = self.flat_list.get(idx) {
                        // Skip group header rows — Enter on a header collapses/expands instead.
                        if matches!(entry.row_kind, FlatEntryKind::GroupHeader { .. }) {
                            let entry = entry.clone();
                            if let FlatEntryKind::GroupHeader {
                                cwd_key, expanded, ..
                            } = &entry.row_kind
                            {
                                let key = cwd_key.clone();
                                self.group_collapsed.insert(key, !expanded);
                                self.rebuild_flat_list();
                            }
                        } else {
                            self.selected_detail = Some(entry.info.clone());
                            self.selected_detail_subtree = Some(entry.subtree_stats);
                            self.active_view = ActiveView::Detail;
                        }
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
            Action::EnterFilter => {
                self.filter_active = true;
                // Starting a free-text search resets the curated filter to All.
                self.focus_filter = FocusFilter::All;
            }
            Action::ClearFilter => {
                self.filter_active = false;
                self.filter_text.clear();
                self.rebuild_flat_list();
            }
            Action::FilterInput(c) => {
                self.filter_text.push(c);
                self.rebuild_flat_list();
            }
            Action::FilterBackspace => {
                self.filter_text.pop();
                if self.filter_text.is_empty() {
                    self.filter_active = false;
                }
                self.rebuild_flat_list();
            }
            Action::ToggleTelemetryView => {
                self.telemetry_view = !self.telemetry_view;
            }
            Action::CycleFocusFilter => {
                self.focus_filter = self.focus_filter.next();
                // Pressing F while free-text filter is active clears the text filter.
                self.filter_active = false;
                self.filter_text.clear();
                self.rebuild_flat_list();
            }
            Action::ToggleProjectGrouping => {
                self.project_grouping = !self.project_grouping;
                self.rebuild_flat_list();
            }
            Action::ToggleHelp => {
                self.help_open = !self.help_open;
            }
            Action::JumpToTerminal => {
                let msg = self.attempt_terminal_jump();
                self.status_message = Some((msg, Instant::now()));
            }
        }
    }

    /// Attempt to focus the terminal pane that owns the currently selected process.
    ///
    /// Returns a short status string suitable for the 3-second flash.
    fn attempt_terminal_jump(&self) -> String {
        use crate::terminals;

        let adapter = match terminals::detect_from_env() {
            Some(a) => a,
            None => {
                return "Cannot jump: terminal not detected (iTerm2/tmux/kitty/wezterm supported)"
                    .to_string();
            }
        };

        let info = match &self.selected_detail {
            Some(i) => i,
            None => return "No process selected".to_string(),
        };

        // Resolve the TTY for the selected process.
        let tty = match resolve_tty(info.pid) {
            Some(t) => t,
            None => return format!("Cannot determine TTY for PID {}", info.pid),
        };

        // Check for self-jump: if the current terminal matches the tty.
        if let Some(current) = adapter.current_target() {
            // Normalise for comparison: strip /dev/ prefix.
            let tty_norm = tty.trim_start_matches("/dev/");
            let current_norm = current.pane_id.trim_start_matches("/dev/");
            if tty_norm == current_norm {
                return "This session is already in focus".to_string();
            }
        }

        match adapter.focus_by_tty(&tty) {
            Ok(pane) => format!("Jumped to {} {}", adapter.name(), pane),
            Err(e) => format!("Failed to jump: {e}"),
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
    /// * `processes`  - Complete flat list of process snapshots from the current refresh.
    /// * `stats`      - System-wide resource snapshot.
    /// * `telemetry`  - Per-PID telemetry map produced by the telemetry pipeline.
    pub fn update_processes(
        &mut self,
        processes: Vec<ProcessInfo>,
        stats: SystemStats,
        telemetry: TelemetryMap,
    ) {
        self.system_stats = stats;
        self.telemetry = telemetry;

        // Snapshot expansion state before rebuilding so the user's open/close choices survive.
        let old_expansion = collect_expansion(&self.forest);

        self.update_history(&processes);

        // Prune history for processes that no longer exist, preventing unbounded growth.
        let live_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
        self.cpu_history.retain(|pid, _| live_pids.contains(pid));
        self.mem_history.retain(|pid, _| live_pids.contains(pid));
        // Prune telemetry alongside cpu/mem history so stale PIDs don't accumulate.
        self.telemetry.retain(|pid, _| live_pids.contains(pid));
        self.cost_burn_history
            .retain(|pid, _| live_pids.contains(pid));

        // Update cost burn tracking for the burn-rate indicator in the agent view.
        // For each PID that now has a cost, record (Instant, cost) if no entry
        // exists yet; otherwise leave the old entry in place so we can compare.
        for (&pid, tel) in &self.telemetry {
            if let Some(cost) = tel.cost_usd {
                self.cost_burn_history
                    .entry(pid)
                    .or_insert_with(|| (Instant::now(), cost));
            }
        }

        self.forest = build_forest(&processes);
        preserve_expansion(&mut self.forest, &old_expansion);

        // Compute idle/active badges for root processes.
        self.update_activity_states();

        // Prune activity history for PIDs that have exited.
        self.aggregate_cpu_history
            .retain(|pid, _| live_pids.contains(pid));
        self.activity_state.retain(|pid, _| live_pids.contains(pid));
        self.activity_state_since
            .retain(|pid, _| live_pids.contains(pid));

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

    /// Rebuild and sort `flat_list`, inject activity badges, apply the current
    /// filter, optionally apply project grouping, then clamp the selection cursor.
    ///
    /// Pipeline order: sort → inject activity → curated focus filter → text
    /// filter → project grouping → clamp selection.
    ///
    /// Call this whenever the forest structure, sort parameters, or filter changes.
    fn rebuild_flat_list(&mut self) {
        self.sort_flat_list();

        // Inject activity state and time-in-state into root FlatEntry values.
        for entry in &mut self.flat_list {
            if entry.is_root {
                entry.activity = self.activity_state.get(&entry.info.pid).copied();
                entry.activity_since = self.activity_state_since.get(&entry.info.pid).copied();
            }
        }

        // Apply curated focus filter (modifies flat_list in place).
        self.apply_focus_filter();

        // Apply free-text filter on top of the focus filter result.
        self.apply_filter();

        // Optionally re-organise into cwd groups with synthetic header rows.
        if self.project_grouping {
            self.apply_project_grouping();
        }

        self.clamp_selection();
    }

    /// Apply the active [`FocusFilter`] to `flat_list`, retaining only entries
    /// that match the predicate (and their ancestors for non-root matches).
    ///
    /// When the filter is [`FocusFilter::All`] this is a no-op.
    fn apply_focus_filter(&mut self) {
        if self.focus_filter == FocusFilter::All {
            return;
        }

        let filter = self.focus_filter;
        let telemetry = &self.telemetry;

        let n = self.flat_list.len();
        let mut keep = vec![false; n];

        for (i, entry) in self.flat_list.iter().enumerate() {
            keep[i] = focus_filter_matches(entry, filter, telemetry);
        }

        // Propagate keep upward through parent chains so matched children pull
        // in their ancestors (same logic as apply_filter).
        for i in (0..n).rev() {
            if !keep[i] {
                continue;
            }
            let child_depth = self.flat_list[i].depth;
            if child_depth == 0 {
                continue;
            }
            for j in (0..i).rev() {
                if self.flat_list[j].depth == child_depth - 1 {
                    keep[j] = true;
                    break;
                }
            }
        }

        let mut idx = 0;
        self.flat_list.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });
    }

    /// Re-organise `flat_list` into cwd-based groups, inserting a synthetic
    /// [`FlatEntryKind::GroupHeader`] row before each group.
    ///
    /// Empty groups (all members filtered out before this step) are skipped
    /// entirely — no ghost headers are emitted.
    ///
    /// Child processes inherit their root's cwd for grouping purposes: a `zsh`
    /// child of a Claude root goes into the same group as the root.
    fn apply_project_grouping(&mut self) {
        // Step 1: collect root pids visible in the current (post-filter) flat_list
        // and map each root's cwd to a canonical group key.
        // Roots without a cwd are placed in a special "" group (rendered as "no project").
        let mut root_cwd: HashMap<u32, String> = HashMap::new();
        for entry in &self.flat_list {
            if entry.is_root {
                let cwd = entry.info.cwd.clone().unwrap_or_default();
                root_cwd.insert(entry.info.pid, cwd);
            }
        }

        // Step 2: for each flat entry, find the group key by walking up to the root.
        // Children inherit the root's cwd.
        let mut entry_group: Vec<String> = Vec::with_capacity(self.flat_list.len());
        {
            // Keep a stack of (depth, cwd_key) for the current path.
            let mut ancestor_stack: Vec<(usize, String)> = Vec::new();
            for entry in &self.flat_list {
                // Pop stack entries that are deeper than this entry's parent.
                while ancestor_stack
                    .last()
                    .is_some_and(|(d, _)| *d >= entry.depth)
                {
                    ancestor_stack.pop();
                }
                if entry.is_root {
                    let cwd = entry.info.cwd.clone().unwrap_or_default();
                    ancestor_stack.push((entry.depth, cwd.clone()));
                    entry_group.push(cwd);
                } else {
                    // Inherit from nearest ancestor root.
                    let inherited = ancestor_stack
                        .iter()
                        .rev()
                        .find(|(d, _)| *d == 0)
                        .map(|(_, k)| k.clone())
                        .unwrap_or_default();
                    entry_group.push(inherited);
                }
            }
        }

        // Step 3: build an ordered list of unique group keys (preserving first appearance
        // order, which is determined by the post-sort post-filter flat_list).
        let mut seen_groups: Vec<String> = Vec::new();
        let mut seen_set: HashSet<String> = HashSet::new();
        for key in &entry_group {
            if seen_set.insert(key.clone()) {
                seen_groups.push(key.clone());
            }
        }

        // Step 4: for each group, compute rollup stats from root entries only.
        let mut group_rollup: HashMap<String, GroupRollup> = HashMap::new();
        for (entry, group_key) in self.flat_list.iter().zip(entry_group.iter()) {
            if !entry.is_root {
                continue;
            }
            let rollup = group_rollup.entry(group_key.clone()).or_default();
            rollup.session_count += 1;
            // Retrieve telemetry for cost and context fraction.
            if let Some(tel) = self.telemetry.get(&entry.info.pid) {
                if let Some(cost) = tel.cost_usd {
                    *rollup.total_cost.get_or_insert(0.0) += cost;
                }
                if let Some(frac) = tel.context_fraction() {
                    rollup.ctx_sum += frac;
                    rollup.ctx_count += 1;
                }
            }
            // Most-recent-active = max(start_time + run_time).
            let activity_ts = entry.info.start_time + entry.info.run_time;
            if activity_ts > rollup.latest_activity {
                rollup.latest_activity = activity_ts;
            }
        }

        // Step 5: sort groups by most-recent-active descending.
        seen_groups.sort_by(|a, b| {
            let ra = group_rollup.get(a).map(|r| r.latest_activity).unwrap_or(0);
            let rb = group_rollup.get(b).map(|r| r.latest_activity).unwrap_or(0);
            rb.cmp(&ra)
        });

        // Step 6: assemble the new flat list with headers.
        let mut new_list: Vec<FlatEntry> =
            Vec::with_capacity(self.flat_list.len() + seen_groups.len());
        let old_list = std::mem::take(&mut self.flat_list);

        for group_key in &seen_groups {
            let rollup = match group_rollup.get(group_key) {
                Some(r) if r.session_count > 0 => r,
                _ => continue,
            };

            let collapsed = self
                .group_collapsed
                .get(group_key)
                .copied()
                .unwrap_or(false);
            let slug = truncate_path_middle(group_key, 50);
            let avg_ctx = if rollup.ctx_count > 0 {
                Some(rollup.ctx_sum / rollup.ctx_count as f32)
            } else {
                None
            };

            // Build a synthetic FlatEntry for the header.
            // `info` is a zeroed-out placeholder; renderers must check `row_kind`.
            let header_entry = FlatEntry {
                info: ProcessInfo {
                    pid: 0,
                    parent_pid: None,
                    name: slug.clone(),
                    cmd: vec![],
                    exe_path: None,
                    cwd: Some(group_key.clone()),
                    cpu_usage: 0.0,
                    memory_bytes: 0,
                    status: String::new(),
                    environ_count: 0,
                    start_time: 0,
                    run_time: 0,
                },
                depth: 0,
                is_root: false,
                expanded: !collapsed,
                has_children: true,
                is_last_sibling: false,
                kind: None,
                subtree_stats: SubtreeStats::default(),
                activity: None,
                activity_since: None,
                row_kind: FlatEntryKind::GroupHeader {
                    slug,
                    session_count: rollup.session_count,
                    total_cost: rollup.total_cost,
                    avg_context_fraction: avg_ctx,
                    expanded: !collapsed,
                    cwd_key: group_key.clone(),
                },
            };
            new_list.push(header_entry);

            if !collapsed {
                // Append all entries that belong to this group.
                for (entry, key) in old_list.iter().zip(entry_group.iter()) {
                    if key == group_key {
                        new_list.push(entry.clone());
                    }
                }
            }
        }

        self.flat_list = new_list;
    }

    /// Update the idle/active classification for every root process in the forest.
    ///
    /// When the state transitions (e.g. `Idle` → `Active` → `Idle`), the
    /// `activity_state_since` timestamp is reset to [`Instant::now`]. If the
    /// state is unchanged the timestamp is left alone so the elapsed duration
    /// continues to grow monotonically — a warning badge fires only after a
    /// *continuous* idle period exceeds the threshold.
    pub fn update_activity_states(&mut self) {
        for root in &self.forest {
            let pid = root.info.pid;
            let buf = self.aggregate_cpu_history.entry(pid).or_default();
            if buf.len() == HISTORY_LEN {
                buf.pop_front();
            }
            buf.push_back(root.info.cpu_usage);

            let new_state = if buf.len() < IDLE_SAMPLE_WINDOW {
                ActivityState::Unknown
            } else {
                let window_start = buf.len() - IDLE_SAMPLE_WINDOW;
                let all_idle = buf
                    .iter()
                    .skip(window_start)
                    .all(|&s| s < IDLE_CPU_THRESHOLD);
                if all_idle {
                    ActivityState::Idle
                } else {
                    ActivityState::Active
                }
            };

            let old_state = self.activity_state.get(&pid).copied();
            if old_state != Some(new_state) {
                // State changed — reset the timer.
                self.activity_state_since.insert(pid, Instant::now());
            }
            // Ensure the map has an entry even on first observation.
            self.activity_state_since
                .entry(pid)
                .or_insert_with(Instant::now);
            self.activity_state.insert(pid, new_state);
        }
    }

    /// Filter `flat_list` in-place based on `filter_text`.
    fn apply_filter(&mut self) {
        if self.filter_text.is_empty() {
            return;
        }

        let query = self.filter_text.to_lowercase();
        let n = self.flat_list.len();

        // Pass 1: direct match flags.
        let mut keep = vec![false; n];
        for (i, entry) in self.flat_list.iter().enumerate() {
            keep[i] = entry_matches_filter(entry, &query);
        }

        // Pass 2: propagate keep upward through parent chains.
        for i in (0..n).rev() {
            if !keep[i] {
                continue;
            }
            let child_depth = self.flat_list[i].depth;
            if child_depth == 0 {
                continue;
            }
            for j in (0..i).rev() {
                if self.flat_list[j].depth == child_depth - 1 {
                    keep[j] = true;
                    break;
                }
            }
        }

        let mut idx = 0;
        self.flat_list.retain(|_| {
            let keep_entry = keep[idx];
            idx += 1;
            keep_entry
        });
    }

    /// Return the number of process rows (non-header) currently visible in `flat_list`.
    ///
    /// Used by the status-bar filter pill to render `(shown/total)`.
    pub fn visible_process_count(&self) -> usize {
        self.flat_list
            .iter()
            .filter(|e| matches!(e.row_kind, FlatEntryKind::Process))
            .count()
    }

    /// Return the total number of process rows in the forest before any filtering.
    ///
    /// Used by the status-bar filter pill denominator.
    pub fn total_process_count(&self) -> usize {
        // The forest only contains roots; traverse to count all nodes.
        count_forest_nodes(&self.forest)
    }

    /// Return the PID of the currently focused process, if any.
    ///
    /// Returns `None` when the focused row is a group header (no process).
    fn selected_pid(&self) -> Option<u32> {
        match self.active_view {
            ActiveView::Tree => {
                let idx = self.table_state.selected()?;
                let entry = self.flat_list.get(idx)?;
                if matches!(entry.row_kind, FlatEntryKind::GroupHeader { .. }) {
                    return None;
                }
                Some(entry.info.pid)
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
    /// * `key` - The raw key event from crossterm.
    /// * `ctx` - Current UI mode context flags.
    pub fn map_key_to_action(key: KeyEvent, ctx: &KeyContext<'_>) -> Option<Action> {
        // Ctrl+C is a universal quit regardless of view or mode.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }

        // Config popup captures all input when open.
        if ctx.config_open {
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
        if ctx.confirming_kill {
            return match key.code {
                KeyCode::Char('y') => Some(Action::ConfirmKill),
                KeyCode::Char('n') | KeyCode::Esc => Some(Action::CancelKill),
                _ => None,
            };
        }

        // Help overlay: Esc closes; any other bound key closes AND executes it.
        // Unbound keys just close.
        if ctx.help_open {
            // Always close the overlay first.
            // Esc closes only.
            if key.code == KeyCode::Esc {
                return Some(Action::ToggleHelp);
            }
            // For other keys, close help then fall through to normal dispatch.
            // We do this by returning ToggleHelp here; the caller must re-process
            // the same key in the next loop iteration. Since that requires more
            // invasive changes to main.rs, we take the simpler approach: return
            // ToggleHelp for Esc, and let any other key close the overlay via
            // a secondary dispatch path (handled in main.rs draw loop).
            return Some(Action::ToggleHelp);
        }

        // Filter bar intercepts most keystrokes when active.
        if ctx.filter_active {
            return match key.code {
                KeyCode::Esc => Some(Action::ClearFilter),
                KeyCode::Backspace => Some(Action::FilterBackspace),
                KeyCode::Up | KeyCode::Char('k') => Some(Action::MoveUp),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::MoveDown),
                KeyCode::Enter => Some(Action::SelectProcess),
                KeyCode::Char(c) => Some(Action::FilterInput(c)),
                _ => None,
            };
        }

        match ctx.active_view {
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
                KeyCode::Char('/') => Some(Action::EnterFilter),
                KeyCode::Char('t') | KeyCode::Char('T') => Some(Action::ToggleTelemetryView),
                // Phase 4 additions
                KeyCode::Char('F') => Some(Action::CycleFocusFilter),
                KeyCode::Char('g') => Some(Action::ToggleProjectGrouping),
                KeyCode::Char('?') => Some(Action::ToggleHelp),
                KeyCode::Char('z') => Some(Action::ClearFilter),
                _ => None,
            },
            ActiveView::Detail => match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Esc => Some(Action::BackToTree),
                KeyCode::Char('x') => Some(Action::KillRequest),
                KeyCode::Char('c') => Some(Action::ToggleConfig),
                KeyCode::Char('t') | KeyCode::Char('T') => Some(Action::ToggleTelemetryView),
                // Phase 4 additions
                KeyCode::Char('?') => Some(Action::ToggleHelp),
                KeyCode::Tab => Some(Action::JumpToTerminal),
                _ => None,
            },
        }
    }
}

/// Count all nodes in the forest (recursively).
fn count_forest_nodes(forest: &[ProcessNode]) -> usize {
    forest
        .iter()
        .map(|n| 1 + count_forest_nodes(&n.children))
        .sum()
}

/// Resolve the controlling TTY for a process by PID.
///
/// Tries `ps -o tty= -p <pid>` which works on both macOS and Linux.
/// Returns `None` when the TTY cannot be determined (e.g. daemon processes with no tty).
fn resolve_tty(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if tty.is_empty() || tty == "?" {
        return None;
    }
    // Prepend /dev/ if not already present.
    if tty.starts_with('/') {
        Some(tty)
    } else {
        Some(format!("/dev/{tty}"))
    }
}

/// Intermediate accumulator used during project-grouping rollup.
#[derive(Default)]
struct GroupRollup {
    session_count: usize,
    total_cost: Option<f64>,
    ctx_sum: f32,
    ctx_count: usize,
    /// Max of `start_time + run_time` across group members.
    latest_activity: u64,
}

/// Truncate a filesystem path to at most `max_chars` characters, replacing the
/// middle with `…` when necessary.
///
/// For example: `/Users/me/very/long/path/to/project` → `/Users/me/…/project`.
fn truncate_path_middle(path: &str, max_chars: usize) -> String {
    if path.len() <= max_chars {
        return path.to_string();
    }
    // Keep a prefix and suffix around an ellipsis.
    let half = (max_chars.saturating_sub(3)) / 2;
    let prefix = &path[..half];
    let suffix = &path[path.len().saturating_sub(half)..];
    format!("{prefix}...{suffix}")
}

/// Returns `true` when `entry` passes the given curated focus filter.
///
/// The telemetry map is consulted for status and context fraction. Entries
/// that lack telemetry are excluded from `Attention` and `HighContext` filters
/// but included in `HighCpu` and `Recent` when their numeric fields qualify.
pub fn focus_filter_matches(
    entry: &FlatEntry,
    filter: FocusFilter,
    telemetry: &TelemetryMap,
) -> bool {
    match filter {
        FocusFilter::All => true,
        FocusFilter::Attention => {
            let tel = telemetry.get(&entry.info.pid);
            let needs_input = tel
                .and_then(|t| t.status)
                .is_some_and(|s| s == AgentStatus::NeedsInput);
            let high_ctx = tel
                .and_then(|t| t.context_fraction())
                .is_some_and(|f| f >= 0.80);
            needs_input || high_ctx
        }
        FocusFilter::HighCpu => entry.info.cpu_usage >= 30.0,
        FocusFilter::HighContext => {
            let tel = telemetry.get(&entry.info.pid);
            tel.and_then(|t| t.context_fraction())
                .is_some_and(|f| f >= 0.80)
        }
        FocusFilter::Recent => entry.info.run_time <= 600,
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

/// Returns `true` when `entry` matches the given lower-cased `query`.
///
/// Checked fields (all compared case-insensitively):
/// - Display name (`filter::display_name`)
/// - PID as a decimal string
/// - The basename of the working directory path
/// - The full command line (argv joined with spaces)
pub fn entry_matches_filter(entry: &FlatEntry, query: &str) -> bool {
    use crate::process::display_name;

    if display_name(&entry.info).to_lowercase().contains(query) {
        return true;
    }

    if entry.info.pid.to_string().contains(query) {
        return true;
    }

    if let Some(ref cwd) = entry.info.cwd {
        let basename = cwd.rsplit('/').next().unwrap_or(cwd.as_str());
        if basename.to_lowercase().contains(query) {
            return true;
        }
    }

    let cmd = entry.info.cmd.join(" ");
    cmd.to_lowercase().contains(query)
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
    use crate::process::{build_forest, flatten_visible, ProcessInfo};

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

    // ── Activity state tests ────────────────────────────────────────────────

    fn app_with_procs(procs: Vec<ProcessInfo>) -> App {
        let mut app = App::new();
        app.forest = build_forest(&procs);
        app.flat_list = flatten_visible(&app.forest);
        app
    }

    fn make_proc_simple(pid: u32, parent: Option<u32>, name: &str, cpu: f32) -> ProcessInfo {
        make_proc(pid, parent, name, cpu, 0)
    }

    #[test]
    fn test_activity_unknown_fewer_than_window() {
        let mut app = App::new();
        let pid = 1u32;
        let procs = vec![make_proc_simple(pid, None, "claude", 0.0)];
        app.forest = build_forest(&procs);
        let buf = app.aggregate_cpu_history.entry(pid).or_default();
        for _ in 0..(IDLE_SAMPLE_WINDOW - 2) {
            buf.push_back(0.0);
        }
        app.update_activity_states();
        assert_eq!(app.activity_state.get(&pid), Some(&ActivityState::Unknown));
    }

    #[test]
    fn test_activity_idle() {
        let mut app = App::new();
        let pid = 1u32;
        let procs = vec![make_proc_simple(pid, None, "claude", 0.1)];
        app.forest = build_forest(&procs);
        let buf = app.aggregate_cpu_history.entry(pid).or_default();
        for _ in 0..(IDLE_SAMPLE_WINDOW - 1) {
            buf.push_back(0.1);
        }
        app.update_activity_states();
        assert_eq!(app.activity_state.get(&pid), Some(&ActivityState::Idle));
    }

    #[test]
    fn test_activity_active() {
        let mut app = App::new();
        let pid = 1u32;
        let procs = vec![make_proc_simple(pid, None, "claude", 50.0)];
        app.forest = build_forest(&procs);
        let buf = app.aggregate_cpu_history.entry(pid).or_default();
        for _ in 0..(IDLE_SAMPLE_WINDOW - 1) {
            buf.push_back(0.1);
        }
        app.update_activity_states();
        assert_eq!(app.activity_state.get(&pid), Some(&ActivityState::Active));
    }

    // ── Filter tests ────────────────────────────────────────────────────────

    #[test]
    fn test_filter_matches_name() {
        let procs = vec![
            make_proc_simple(1, None, "claude", 0.0),
            make_proc_simple(2, None, "codex", 0.0),
        ];
        let mut app = app_with_procs(procs);
        app.filter_text = "claude".to_string();
        app.apply_filter();
        assert_eq!(app.flat_list.len(), 1);
        assert_eq!(app.flat_list[0].info.name, "claude");
    }

    #[test]
    fn test_filter_preserves_parents() {
        let procs = vec![
            make_proc_simple(1, None, "claude", 0.0),
            make_proc_simple(2, Some(1), "node", 0.0),
        ];
        let mut app = app_with_procs(procs);
        app.filter_text = "node".to_string();
        app.apply_filter();
        assert_eq!(app.flat_list.len(), 2);
        assert!(app.flat_list.iter().any(|e| e.info.pid == 1));
        assert!(app.flat_list.iter().any(|e| e.info.pid == 2));
    }

    #[test]
    fn test_filter_case_insensitive() {
        let procs = vec![make_proc_simple(1, None, "claude", 0.0)];
        let mut app = app_with_procs(procs);
        app.filter_text = "CLAUDE".to_string();
        app.apply_filter();
        assert_eq!(app.flat_list.len(), 1);
    }

    #[test]
    fn test_filter_clears() {
        let procs = vec![
            make_proc_simple(1, None, "claude", 0.0),
            make_proc_simple(2, None, "codex", 0.0),
        ];
        let mut app = app_with_procs(procs);
        app.filter_text = "claude".to_string();
        app.apply_filter();
        assert_eq!(app.flat_list.len(), 1);
        app.handle_action(Action::ClearFilter);
        assert_eq!(app.flat_list.len(), 2);
    }

    // ── Focus filter predicate tests ────────────────────────────────────────

    fn make_entry_with(cpu: f32, run_time: u64) -> FlatEntry {
        let info = make_proc(42, None, "claude", cpu, 0);
        FlatEntry {
            info: ProcessInfo { run_time, ..info },
            depth: 0,
            is_root: true,
            expanded: true,
            has_children: false,
            is_last_sibling: true,
            kind: None,
            subtree_stats: SubtreeStats::default(),
            activity: None,
            activity_since: None,
            row_kind: FlatEntryKind::Process,
        }
    }

    #[test]
    fn focus_filter_all_includes_everything() {
        let entry = make_entry_with(0.0, 9999);
        let telemetry = TelemetryMap::new();
        assert!(focus_filter_matches(&entry, FocusFilter::All, &telemetry));
    }

    #[test]
    fn focus_filter_high_cpu_above_threshold() {
        let entry = make_entry_with(35.0, 0);
        let telemetry = TelemetryMap::new();
        assert!(focus_filter_matches(
            &entry,
            FocusFilter::HighCpu,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_high_cpu_below_threshold() {
        let entry = make_entry_with(10.0, 0);
        let telemetry = TelemetryMap::new();
        assert!(!focus_filter_matches(
            &entry,
            FocusFilter::HighCpu,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_recent_within_10_minutes() {
        let entry = make_entry_with(0.0, 300); // 5 minutes
        let telemetry = TelemetryMap::new();
        assert!(focus_filter_matches(
            &entry,
            FocusFilter::Recent,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_recent_outside_10_minutes() {
        let entry = make_entry_with(0.0, 700); // > 10 minutes
        let telemetry = TelemetryMap::new();
        assert!(!focus_filter_matches(
            &entry,
            FocusFilter::Recent,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_attention_needs_input() {
        use crate::telemetry::AgentTelemetry;
        let entry = make_entry_with(0.0, 0);
        let mut telemetry = TelemetryMap::new();
        telemetry.insert(
            42,
            AgentTelemetry {
                status: Some(AgentStatus::NeedsInput),
                ..Default::default()
            },
        );
        assert!(focus_filter_matches(
            &entry,
            FocusFilter::Attention,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_attention_high_context() {
        use crate::telemetry::AgentTelemetry;
        let entry = make_entry_with(0.0, 0);
        let mut telemetry = TelemetryMap::new();
        telemetry.insert(
            42,
            AgentTelemetry {
                context_tokens: Some(850),
                context_window: Some(1000),
                ..Default::default()
            },
        );
        assert!(focus_filter_matches(
            &entry,
            FocusFilter::Attention,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_high_context_triggers_at_80_percent() {
        use crate::telemetry::AgentTelemetry;
        let entry = make_entry_with(0.0, 0);
        let mut telemetry = TelemetryMap::new();
        telemetry.insert(
            42,
            AgentTelemetry {
                context_tokens: Some(800),
                context_window: Some(1000),
                ..Default::default()
            },
        );
        assert!(focus_filter_matches(
            &entry,
            FocusFilter::HighContext,
            &telemetry
        ));
    }

    #[test]
    fn focus_filter_high_context_below_80_percent() {
        use crate::telemetry::AgentTelemetry;
        let entry = make_entry_with(0.0, 0);
        let mut telemetry = TelemetryMap::new();
        telemetry.insert(
            42,
            AgentTelemetry {
                context_tokens: Some(700),
                context_window: Some(1000),
                ..Default::default()
            },
        );
        assert!(!focus_filter_matches(
            &entry,
            FocusFilter::HighContext,
            &telemetry
        ));
    }

    // ── FocusFilter cycle tests ──────────────────────────────────────────────

    #[test]
    fn focus_filter_cycles_through_all_variants() {
        let mut f = FocusFilter::All;
        f = f.next();
        assert_eq!(f, FocusFilter::Attention);
        f = f.next();
        assert_eq!(f, FocusFilter::HighCpu);
        f = f.next();
        assert_eq!(f, FocusFilter::HighContext);
        f = f.next();
        assert_eq!(f, FocusFilter::Recent);
        f = f.next();
        assert_eq!(f, FocusFilter::All); // wraps
    }

    // ── Project grouping rollup math tests ───────────────────────────────────

    #[test]
    fn project_grouping_inserts_headers() {
        let procs = vec![
            ProcessInfo {
                pid: 1,
                parent_pid: None,
                name: "claude".to_string(),
                cmd: vec!["claude".to_string()],
                exe_path: None,
                cwd: Some("/home/user/proj-a".to_string()),
                cpu_usage: 5.0,
                memory_bytes: 100,
                status: "Run".to_string(),
                environ_count: 0,
                start_time: 0,
                run_time: 0,
            },
            ProcessInfo {
                pid: 2,
                parent_pid: None,
                name: "claude".to_string(),
                cmd: vec!["claude".to_string()],
                exe_path: None,
                cwd: Some("/home/user/proj-b".to_string()),
                cpu_usage: 2.0,
                memory_bytes: 50,
                status: "Run".to_string(),
                environ_count: 0,
                start_time: 0,
                run_time: 0,
            },
        ];
        let mut app = app_with_procs(procs);
        app.project_grouping = true;
        app.rebuild_flat_list();

        // With two distinct cwds there should be 2 headers + 2 process rows = 4.
        assert_eq!(
            app.flat_list.len(),
            4,
            "expected 2 headers + 2 processes, got {}",
            app.flat_list.len()
        );
        let headers: Vec<_> = app
            .flat_list
            .iter()
            .filter(|e| matches!(e.row_kind, FlatEntryKind::GroupHeader { .. }))
            .collect();
        assert_eq!(headers.len(), 2);
    }

    #[test]
    fn project_grouping_same_cwd_shares_header() {
        let procs = vec![
            ProcessInfo {
                pid: 1,
                parent_pid: None,
                name: "claude".to_string(),
                cmd: vec!["claude".to_string()],
                exe_path: None,
                cwd: Some("/shared/proj".to_string()),
                cpu_usage: 1.0,
                memory_bytes: 10,
                status: "Run".to_string(),
                environ_count: 0,
                start_time: 0,
                run_time: 0,
            },
            ProcessInfo {
                pid: 2,
                parent_pid: None,
                name: "claude".to_string(),
                cmd: vec!["claude".to_string()],
                exe_path: None,
                cwd: Some("/shared/proj".to_string()),
                cpu_usage: 2.0,
                memory_bytes: 20,
                status: "Run".to_string(),
                environ_count: 0,
                start_time: 0,
                run_time: 0,
            },
        ];
        let mut app = app_with_procs(procs);
        app.project_grouping = true;
        app.rebuild_flat_list();

        // Same cwd → 1 header + 2 processes = 3 rows.
        assert_eq!(app.flat_list.len(), 3);
        let headers: Vec<_> = app
            .flat_list
            .iter()
            .filter(|e| matches!(e.row_kind, FlatEntryKind::GroupHeader { .. }))
            .collect();
        assert_eq!(headers.len(), 1);

        // Header should count both sessions.
        if let FlatEntryKind::GroupHeader { session_count, .. } = &headers[0].row_kind {
            assert_eq!(*session_count, 2);
        }
    }

    // ── truncate_path_middle tests ───────────────────────────────────────────

    #[test]
    fn truncate_path_short_path_unchanged() {
        assert_eq!(truncate_path_middle("/short", 50), "/short");
    }

    #[test]
    fn truncate_path_long_path_truncated() {
        let long = "/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t/u/v/w/x/y/z";
        let result = truncate_path_middle(long, 20);
        assert!(
            result.len() <= 23,
            "truncated should be close to max: {result}"
        );
        assert!(result.contains("..."));
    }

    // ── Key routing tests ────────────────────────────────────────────────────

    fn tree_ctx() -> KeyContext<'static> {
        // We need a static reference for active_view in KeyContext.
        // Use a Box::leak trick in tests only.
        static TREE: ActiveView = ActiveView::Tree;
        KeyContext {
            active_view: &TREE,
            confirming_kill: false,
            config_open: false,
            filter_active: false,
            help_open: false,
        }
    }

    fn detail_ctx() -> KeyContext<'static> {
        static DETAIL: ActiveView = ActiveView::Detail;
        KeyContext {
            active_view: &DETAIL,
            confirming_kill: false,
            config_open: false,
            filter_active: false,
            help_open: false,
        }
    }

    #[test]
    fn key_f_cycles_focus_filter_in_tree() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let key = KeyEvent {
            code: KeyCode::Char('F'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = App::map_key_to_action(key, &tree_ctx());
        assert!(
            matches!(action, Some(Action::CycleFocusFilter)),
            "F in tree should map to CycleFocusFilter"
        );
    }

    #[test]
    fn key_g_toggles_grouping_in_tree() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let key = KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = App::map_key_to_action(key, &tree_ctx());
        assert!(matches!(action, Some(Action::ToggleProjectGrouping)));
    }

    #[test]
    fn key_question_opens_help_in_tree() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let key = KeyEvent {
            code: KeyCode::Char('?'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = App::map_key_to_action(key, &tree_ctx());
        assert!(matches!(action, Some(Action::ToggleHelp)));
    }

    #[test]
    fn key_tab_in_detail_view_triggers_jump() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let key = KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = App::map_key_to_action(key, &detail_ctx());
        assert!(matches!(action, Some(Action::JumpToTerminal)));
    }

    #[test]
    fn key_tab_in_tree_view_cycles_sort() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let key = KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = App::map_key_to_action(key, &tree_ctx());
        assert!(matches!(action, Some(Action::SortNext)));
    }

    #[test]
    fn handle_cycle_focus_filter_resets_text_filter() {
        let procs = vec![make_proc_simple(1, None, "claude", 50.0)];
        let mut app = app_with_procs(procs);
        app.filter_text = "something".to_string();
        app.filter_active = true;
        app.handle_action(Action::CycleFocusFilter);
        assert!(app.filter_text.is_empty(), "text filter should be cleared");
        assert!(!app.filter_active);
        assert_eq!(app.focus_filter, FocusFilter::Attention);
    }

    #[test]
    fn enter_filter_resets_focus_filter() {
        let procs = vec![make_proc_simple(1, None, "claude", 0.0)];
        let mut app = app_with_procs(procs);
        app.focus_filter = FocusFilter::HighCpu;
        app.handle_action(Action::EnterFilter);
        assert_eq!(
            app.focus_filter,
            FocusFilter::All,
            "EnterFilter should reset curated filter"
        );
    }
}
