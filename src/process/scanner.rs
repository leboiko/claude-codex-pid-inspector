use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::info::ProcessInfo;

/// Wraps a [`sysinfo::System`] handle and provides incremental process refresh.
///
/// CPU usage in `sysinfo` is a delta measured between two successive calls.
/// [`ProcessScanner::new`] performs an initial silent refresh so that the very
/// first call to [`ProcessScanner::refresh`] returns meaningful CPU figures
/// rather than all-zero values.
pub struct ProcessScanner {
    system: System,
}

/// The [`ProcessRefreshKind`] used on every refresh cycle.
///
/// We always want CPU, memory, exe path, cmd args, cwd, and environ count.
/// `UpdateKind::OnlyIfNotSet` avoids re-reading fields that rarely change
/// (exe, cwd, cmd) on every tick, keeping refresh overhead low.
fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_cwd(UpdateKind::OnlyIfNotSet)
        .with_environ(UpdateKind::OnlyIfNotSet)
}

impl ProcessScanner {
    /// Create a new [`ProcessScanner`] and perform an initial process refresh.
    ///
    /// The initial refresh seeds the CPU usage counters so that the first
    /// call to [`refresh`](ProcessScanner::refresh) returns non-zero deltas.
    pub fn new() -> Self {
        let mut system = System::new();
        // Seed CPU delta counters; the returned data is discarded.
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        Self { system }
    }

    /// Refresh all process data and return a snapshot `Vec<ProcessInfo>`.
    ///
    /// Dead processes are removed automatically (`remove_dead_processes = true`).
    /// The returned `Vec` is re-allocated on every call; callers should diff or
    /// replace their cached list rather than accumulating snapshots.
    pub fn refresh(&mut self) -> Vec<ProcessInfo> {
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());

        self.system
            .processes()
            .iter()
            // `pid` is a `&Pid` (reference to newtype); `*pid` dereferences to `Pid`.
            .map(|(&pid, proc)| ProcessInfo::from_sysinfo(pid, proc))
            .collect()
    }
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self::new()
    }
}
