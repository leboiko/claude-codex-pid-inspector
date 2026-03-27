use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::info::ProcessInfo;

/// System-wide resource snapshot.
#[derive(Debug, Clone, Default)]
pub struct SystemStats {
    pub cpu_usage: f32,
    pub total_memory: u64,
    pub used_memory: u64,
    pub total_swap: u64,
    pub used_swap: u64,
    pub cpu_count: usize,
}

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
        system.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        Self { system }
    }

    /// Refresh all process and system data, returning both.
    pub fn refresh(&mut self) -> (Vec<ProcessInfo>, SystemStats) {
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());

        let processes = self
            .system
            .processes()
            .iter()
            .map(|(&pid, proc)| ProcessInfo::from_sysinfo(pid, proc))
            .collect();

        let stats = SystemStats {
            cpu_usage: self.system.global_cpu_usage(),
            total_memory: self.system.total_memory(),
            used_memory: self.system.used_memory(),
            total_swap: self.system.total_swap(),
            used_swap: self.system.used_swap(),
            cpu_count: self.system.cpus().len(),
        };

        (processes, stats)
    }
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self::new()
    }
}
