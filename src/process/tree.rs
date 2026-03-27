use std::collections::HashMap;

use super::filter::{is_target_process, process_kind, ProcessKind};
use super::info::ProcessInfo;

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
    processes
        .iter()
        .filter(|p| is_target_process(p))
        .map(|p| build_node(p, &children_map, 0, true))
        .collect()
}

/// Recursively build a [`ProcessNode`], attaching child subtrees.
///
/// Recursion depth is bounded by the OS process tree depth, which is
/// typically shallow (< 20 levels). No cycle guard is needed because
/// the kernel guarantees acyclic parent-child relationships.
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
    }
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
}
