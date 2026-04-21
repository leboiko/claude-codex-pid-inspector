use std::collections::HashMap;
use std::time::Instant;

use super::filter::{is_target_process, process_kind, ActivityState, ProcessKind};
use super::info::ProcessInfo;

/// Aggregated resource usage for a process and all of its descendants.
///
/// Computed by [`compute_subtree_stats`] after the forest is built, so every
/// root node carries the rolled-up totals for its entire subtree.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SubtreeStats {
    /// Sum of CPU usage (%) for this process and all descendants.
    pub total_cpu: f32,
    /// Sum of resident memory (bytes) for this process and all descendants.
    pub total_memory: u64,
    /// Total number of processes in the subtree, including self.
    pub process_count: usize,
}

/// A node in the process tree, holding one process and its direct children.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessNode {
    /// Snapshot data for this process.
    pub info: ProcessInfo,
    /// Direct child processes (recursively nested).
    pub children: Vec<ProcessNode>,
    /// Distance from the tree root (root nodes have depth 0).
    pub depth: usize,
    /// Whether child processes are currently shown in the flattened view.
    pub expanded: bool,
    /// `true` for nodes that are top-level targets (not merely child subtrees).
    pub is_root: bool,
    /// Aggregated CPU, memory, and count for this node and all descendants.
    pub subtree_stats: SubtreeStats,
}

/// A single row in the flat, scrollable list derived from [`ProcessNode`] trees.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatEntry {
    /// Owned snapshot for this row.
    pub info: ProcessInfo,
    /// Indentation depth.
    pub depth: usize,
    /// `true` when this entry has no parent in the visible forest.
    pub is_root: bool,
    /// Mirror of [`ProcessNode::expanded`].
    pub expanded: bool,
    /// `true` when this node has at least one child process.
    pub has_children: bool,
    /// `true` when this is the last sibling within its parent's child list.
    pub is_last_sibling: bool,
    /// Detected kind (Claude / Codex), or `None` for plain child processes.
    pub kind: Option<ProcessKind>,
    /// Aggregated CPU, memory, and count copied from the corresponding [`ProcessNode`].
    pub subtree_stats: SubtreeStats,
    /// Activity state for root agent processes; `None` for non-root entries.
    /// Injected by `App::rebuild_flat_list` after flattening.
    pub activity: Option<ActivityState>,
    /// The instant at which the current activity state began, for root entries.
    ///
    /// `None` for non-root entries and for roots where the state has not yet
    /// been recorded (e.g. the very first tick). Injected by
    /// `App::rebuild_flat_list` alongside `activity`.
    pub activity_since: Option<Instant>,
}

// ---------------------------------------------------------------------------
// Forest construction
// ---------------------------------------------------------------------------

/// Build a forest of [`ProcessNode`] trees from a flat process snapshot list.
///
/// Only target processes (Claude / Codex, identified by [`is_target_process`])
/// become root nodes. All processes are then considered as potential children
/// when their `parent_pid` matches any node already in the forest.
///
/// Subtree statistics are computed for every root after the tree is assembled,
/// so each node's [`ProcessNode::subtree_stats`] is fully populated.
///
/// # Arguments
///
/// * `processes` - Full slice of process snapshots from the current refresh cycle.
///
/// # Returns
///
/// A `Vec` of root-level [`ProcessNode`] values, each potentially containing a
/// recursive `children` subtree.
pub fn build_forest(processes: &[ProcessInfo]) -> Vec<ProcessNode> {
    // Map parent_pid -> list of child ProcessInfo refs for O(1) child lookup.
    let mut children_map: HashMap<u32, Vec<&ProcessInfo>> = HashMap::new();
    for p in processes {
        if let Some(ppid) = p.parent_pid {
            children_map.entry(ppid).or_default().push(p);
        }
    }

    // Only target processes become tree roots.
    let mut roots: Vec<ProcessNode> = processes
        .iter()
        .filter(|p| is_target_process(p))
        .map(|p| build_node(p, &children_map, 0, true))
        .collect();

    // Compute aggregated stats bottom-up for every root subtree.
    for root in roots.iter_mut() {
        compute_subtree_stats(root);
    }

    roots
}

/// Recursively build a [`ProcessNode`], attaching child subtrees.
///
/// Recursion depth is bounded by the OS process tree depth, which is
/// typically shallow (< 20 levels). No cycle guard is needed because
/// the kernel guarantees acyclic parent-child relationships.
///
/// Note: [`SubtreeStats`] is left at `Default` here; [`compute_subtree_stats`]
/// fills it in a second pass after the entire tree is assembled.
fn build_node<'a>(
    info: &'a ProcessInfo,
    children_map: &HashMap<u32, Vec<&'a ProcessInfo>>,
    depth: usize,
    is_root: bool,
) -> ProcessNode {
    let children = children_map
        .get(&info.pid)
        .map(|kids| {
            kids.iter()
                .map(|child| build_node(child, children_map, depth + 1, false))
                .collect()
        })
        .unwrap_or_default();

    ProcessNode {
        info: info.clone(),
        children,
        depth,
        // Default to expanded so the tree is fully visible on first render.
        expanded: true,
        is_root,
        subtree_stats: SubtreeStats::default(),
    }
}

/// Recursively compute [`SubtreeStats`] for `node` and all of its descendants.
///
/// This is a post-order traversal: children are processed first so that
/// their stats are available when computing the parent's aggregate.
///
/// # Arguments
///
/// * `node` - The node whose subtree stats should be populated (mutated in place).
pub fn compute_subtree_stats(node: &mut ProcessNode) {
    // Recurse into children first (post-order) so their stats are ready.
    for child in node.children.iter_mut() {
        compute_subtree_stats(child);
    }

    // Self contribution.
    let mut stats = SubtreeStats {
        total_cpu: node.info.cpu_usage,
        total_memory: node.info.memory_bytes,
        process_count: 1,
    };

    // Accumulate each child's already-computed subtree.
    for child in &node.children {
        stats.total_cpu += child.subtree_stats.total_cpu;
        stats.total_memory += child.subtree_stats.total_memory;
        stats.process_count += child.subtree_stats.process_count;
    }

    node.subtree_stats = stats;
}

// ---------------------------------------------------------------------------
// Flattening
// ---------------------------------------------------------------------------

/// Flatten the forest into an ordered list of visible rows.
///
/// Collapsed nodes' children are skipped entirely, matching typical tree-view
/// behaviour. The returned `Vec` is in display order (parent before children).
pub fn flatten_visible(forest: &[ProcessNode]) -> Vec<FlatEntry> {
    let mut out = Vec::new();
    let last_idx = forest.len().saturating_sub(1);
    for (i, node) in forest.iter().enumerate() {
        flatten_node(node, &mut out, i == last_idx);
    }
    out
}

/// Recursively push a node (and its visible descendants) onto `out`.
fn flatten_node(node: &ProcessNode, out: &mut Vec<FlatEntry>, is_last_sibling: bool) {
    out.push(FlatEntry {
        info: node.info.clone(),
        depth: node.depth,
        is_root: node.is_root,
        expanded: node.expanded,
        has_children: !node.children.is_empty(),
        is_last_sibling,
        kind: process_kind(&node.info),
        // Copy the pre-computed aggregate so the flat list can render rollups
        // and the detail view can display subtree totals without re-traversing.
        subtree_stats: node.subtree_stats,
        // Activity and activity_since are injected by App::rebuild_flat_list after flattening.
        activity: None,
        activity_since: None,
    });

    if node.expanded {
        let last_child = node.children.len().saturating_sub(1);
        for (i, child) in node.children.iter().enumerate() {
            flatten_node(child, out, i == last_child);
        }
    }
}

// ---------------------------------------------------------------------------
// Expansion state helpers
// ---------------------------------------------------------------------------

/// Toggle the `expanded` flag on the node whose pid matches `target_pid`.
///
/// Performs a depth-first search through the entire forest.
pub fn toggle_expand(forest: &mut [ProcessNode], target_pid: u32) {
    for node in forest.iter_mut() {
        if node.info.pid == target_pid {
            node.expanded = !node.expanded;
            return;
        }
        toggle_expand(&mut node.children, target_pid);
    }
}

/// Snapshot the current pid → expanded state for every node in the forest.
///
/// Use this before rebuilding the forest so that [`preserve_expansion`] can
/// restore the UI state after a data refresh.
pub fn collect_expansion(forest: &[ProcessNode]) -> HashMap<u32, bool> {
    let mut map = HashMap::new();
    for node in forest {
        map.insert(node.info.pid, node.expanded);
        // Recurse into children, merging their entries directly into `map`.
        map.extend(collect_expansion(&node.children));
    }
    map
}

/// Restore expansion states from a previously collected pid → bool map.
///
/// Nodes whose pid is absent from `old_states` are left at their current value
/// (defaulting to expanded for newly appearing processes).
pub fn preserve_expansion(forest: &mut [ProcessNode], old_states: &HashMap<u32, bool>) {
    for node in forest.iter_mut() {
        if let Some(&was_expanded) = old_states.get(&node.info.pid) {
            node.expanded = was_expanded;
        }
        preserve_expansion(&mut node.children, old_states);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessInfo;

    fn proc(pid: u32, parent: Option<u32>, name: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid: parent,
            name: name.to_string(),
            cmd: vec![name.to_string()],
            exe_path: None,
            cwd: None,
            cpu_usage: 0.0,
            memory_bytes: 0,
            status: "Run".to_string(),
            environ_count: 0,
            start_time: 0,
            run_time: 0,
        }
    }

    /// Build a `ProcessInfo` with explicit CPU and memory values for stats tests.
    fn proc_with_resources(
        pid: u32,
        parent: Option<u32>,
        name: &str,
        cpu: f32,
        mem: u64,
    ) -> ProcessInfo {
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
    fn build_forest_finds_roots() {
        let procs = vec![
            proc(1, None, "claude"),
            proc(2, Some(1), "node"),
            proc(3, None, "bash"),
        ];
        let forest = build_forest(&procs);
        // Only the claude root should appear; bash is not a target process.
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].info.pid, 1);
        assert_eq!(forest[0].children.len(), 1);
        assert_eq!(forest[0].children[0].info.pid, 2);
    }

    #[test]
    fn flatten_respects_expansion() {
        let procs = vec![proc(1, None, "claude"), proc(2, Some(1), "node")];
        let mut forest = build_forest(&procs);
        let flat = flatten_visible(&forest);
        // Both nodes visible when expanded (the default).
        assert_eq!(flat.len(), 2);

        toggle_expand(&mut forest, 1);
        let flat = flatten_visible(&forest);
        // Only the root visible when collapsed.
        assert_eq!(flat.len(), 1);
    }

    #[test]
    fn collect_and_preserve_expansion() {
        let procs = vec![proc(1, None, "claude"), proc(2, Some(1), "node")];
        let mut forest = build_forest(&procs);
        toggle_expand(&mut forest, 1);

        let states = collect_expansion(&forest);
        assert_eq!(states.get(&1), Some(&false));

        let mut new_forest = build_forest(&procs);
        preserve_expansion(&mut new_forest, &states);
        assert!(!new_forest[0].expanded);
    }

    #[test]
    fn empty_process_list() {
        let forest = build_forest(&[]);
        assert!(forest.is_empty());
        let flat = flatten_visible(&forest);
        assert!(flat.is_empty());
    }

    // -------------------------------------------------------------------------
    // SubtreeStats tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_subtree_stats_leaf_node() {
        // A single root with no children: stats equal the node itself.
        let procs = vec![proc_with_resources(1, None, "claude", 2.5, 1024)];
        let forest = build_forest(&procs);
        let stats = forest[0].subtree_stats;
        assert_eq!(stats.process_count, 1);
        assert!((stats.total_cpu - 2.5).abs() < 1e-4, "cpu mismatch");
        assert_eq!(stats.total_memory, 1024);
    }

    #[test]
    fn test_subtree_stats_with_children() {
        // Root (cpu=1.0, mem=100) + two children (cpu=2.0/3.0, mem=200/300).
        let procs = vec![
            proc_with_resources(1, None, "claude", 1.0, 100),
            proc_with_resources(2, Some(1), "node", 2.0, 200),
            proc_with_resources(3, Some(1), "node", 3.0, 300),
        ];
        let forest = build_forest(&procs);
        let stats = forest[0].subtree_stats;
        assert_eq!(stats.process_count, 3);
        assert!((stats.total_cpu - 6.0).abs() < 1e-4, "cpu mismatch");
        assert_eq!(stats.total_memory, 600);
    }

    #[test]
    fn test_subtree_stats_deep_tree() {
        // Three levels: root -> child -> grandchild.
        let procs = vec![
            proc_with_resources(1, None, "claude", 1.0, 10),
            proc_with_resources(2, Some(1), "node", 1.0, 20),
            proc_with_resources(3, Some(2), "node", 1.0, 30),
        ];
        let forest = build_forest(&procs);
        // Root should aggregate all three levels.
        let root_stats = forest[0].subtree_stats;
        assert_eq!(root_stats.process_count, 3);
        assert!((root_stats.total_cpu - 3.0).abs() < 1e-4, "root cpu");
        assert_eq!(root_stats.total_memory, 60);

        // The middle node's subtree covers itself + grandchild only.
        let child_stats = forest[0].children[0].subtree_stats;
        assert_eq!(child_stats.process_count, 2);
        assert!((child_stats.total_cpu - 2.0).abs() < 1e-4, "child cpu");
        assert_eq!(child_stats.total_memory, 50);
    }

    #[test]
    fn test_subtree_stats_in_flat_entry() {
        // Verify that flatten_visible copies subtree_stats from the node.
        let procs = vec![
            proc_with_resources(1, None, "claude", 4.0, 400),
            proc_with_resources(2, Some(1), "node", 1.0, 100),
        ];
        let forest = build_forest(&procs);
        let flat = flatten_visible(&forest);

        // The root FlatEntry should carry the aggregated stats.
        let root_flat = flat
            .iter()
            .find(|e| e.info.pid == 1)
            .expect("root not found");
        assert_eq!(root_flat.subtree_stats.process_count, 2);
        assert!((root_flat.subtree_stats.total_cpu - 5.0).abs() < 1e-4);
        assert_eq!(root_flat.subtree_stats.total_memory, 500);

        // The child FlatEntry's subtree_stats should equal itself (leaf).
        let child_flat = flat
            .iter()
            .find(|e| e.info.pid == 2)
            .expect("child not found");
        assert_eq!(child_flat.subtree_stats.process_count, 1);
        assert!((child_flat.subtree_stats.total_cpu - 1.0).abs() < 1e-4);
        assert_eq!(child_flat.subtree_stats.total_memory, 100);
    }
}
