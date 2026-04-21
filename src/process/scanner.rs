use std::collections::{HashMap, HashSet, VecDeque};

use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
};

use super::info::ProcessInfo;

/// Number of CPU samples kept in the rolling average window per PID.
const CPU_WINDOW: usize = 3;

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
///
/// A 3-sample rolling average is maintained per PID to smooth the noisy
/// per-tick readings that `sysinfo` produces. See [`ProcessInfo::cpu_usage`]
/// for details on the smoothed value.
pub struct ProcessScanner {
    system: System,
    /// Per-PID rolling window of raw CPU samples (up to [`CPU_WINDOW`] entries).
    cpu_windows: HashMap<u32, VecDeque<f32>>,
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
        Self {
            system,
            cpu_windows: HashMap::new(),
        }
    }

    /// Refresh all process and system data, returning both.
    ///
    /// CPU values in the returned [`ProcessInfo`] list are 3-sample rolling
    /// averages rather than the raw per-tick reading, reducing noise in
    /// sparklines and idle/active classification.
    pub fn refresh(&mut self) -> (Vec<ProcessInfo>, SystemStats) {
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());

        let mut processes: Vec<ProcessInfo> = self
            .system
            .processes()
            .iter()
            .map(|(&pid, proc)| ProcessInfo::from_sysinfo(pid, proc))
            .collect();

        // Update rolling CPU windows and smooth the readings in place.
        let live_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();

        for proc in &mut processes {
            let window = self.cpu_windows.entry(proc.pid).or_default();
            if window.len() == CPU_WINDOW {
                window.pop_front();
            }
            window.push_back(proc.cpu_usage);
            // Replace the raw reading with the window mean.
            let sum: f32 = window.iter().sum();
            proc.cpu_usage = sum / window.len() as f32;
        }

        // Prune CPU windows for processes that have exited; prevents unbounded growth.
        self.cpu_windows.retain(|pid, _| live_pids.contains(pid));

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate the rolling average logic directly — no sysinfo needed.
    fn rolling_average(samples: &[f32]) -> f32 {
        let mut window: VecDeque<f32> = VecDeque::new();
        let mut last_avg = 0.0f32;
        for &s in samples {
            if window.len() == CPU_WINDOW {
                window.pop_front();
            }
            window.push_back(s);
            last_avg = window.iter().sum::<f32>() / window.len() as f32;
        }
        last_avg
    }

    #[test]
    fn three_samples_yields_correct_average() {
        // [0, 30, 60] → mean = 30.0
        let avg = rolling_average(&[0.0, 30.0, 60.0]);
        assert!((avg - 30.0).abs() < 1e-4, "expected 30.0, got {avg}");
    }

    #[test]
    fn fourth_sample_pushes_out_first() {
        // [0, 30, 60, 90] → window after 4th push: [30, 60, 90] → mean = 60.0
        let avg = rolling_average(&[0.0, 30.0, 60.0, 90.0]);
        assert!((avg - 60.0).abs() < 1e-4, "expected 60.0, got {avg}");
    }

    #[test]
    fn single_sample_returns_itself() {
        let avg = rolling_average(&[42.0]);
        assert!((avg - 42.0).abs() < 1e-4);
    }

    #[test]
    fn two_samples_averages_two() {
        let avg = rolling_average(&[10.0, 20.0]);
        assert!((avg - 15.0).abs() < 1e-4);
    }
}
