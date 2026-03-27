use sysinfo::{Pid, Process};

/// A point-in-time snapshot of a single OS process.
///
/// All fields are owned so the struct can be freely moved across threads
/// or stored without worrying about the lifetime of the underlying `sysinfo`
/// data structures.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Numeric process identifier.
    pub pid: u32,
    /// PID of the parent process, if one exists.
    pub parent_pid: Option<u32>,
    /// Short executable name (e.g. `"claude"`, `"node"`).
    pub name: String,
    /// Full command line, split into argv tokens.
    pub cmd: Vec<String>,
    /// Absolute path to the executable on disk, if available.
    pub exe_path: Option<String>,
    /// Working directory of the process, if available.
    pub cwd: Option<String>,
    /// CPU usage as a percentage (0.0 – 100.0 × core-count).
    pub cpu_usage: f32,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
    /// Human-readable process status string (e.g. `"Run"`, `"Sleep"`).
    pub status: String,
    /// Number of environment variables visible to the process.
    pub environ_count: usize,
    /// Unix timestamp (seconds since epoch) at which the process started.
    pub start_time: u64,
    /// Seconds the process has been running.
    pub run_time: u64,
}

impl ProcessInfo {
    /// Build a [`ProcessInfo`] snapshot from a live `sysinfo` process entry.
    ///
    /// # Arguments
    ///
    /// * `pid`  - The sysinfo [`Pid`] for this process.
    /// * `proc` - Reference to the live [`Process`] value owned by [`sysinfo::System`].
    ///
    /// # Returns
    ///
    /// An owned [`ProcessInfo`] snapshot suitable for storage and cross-thread transfer.
    pub fn from_sysinfo(pid: Pid, proc: &Process) -> Self {
        // OsStr / OsString → String conversion: lossy-replace invalid UTF-8 sequences.
        // Most process names and paths are ASCII in practice; lossy conversion keeps
        // things simple without panicking on exotic byte sequences.
        let name = proc.name().to_string_lossy().into_owned();

        let cmd = proc
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();

        let exe_path = proc.exe().map(|p| p.to_string_lossy().into_owned());

        let cwd = proc.cwd().map(|p| p.to_string_lossy().into_owned());

        // sysinfo exposes Pid as a newtype; `.as_u32()` extracts the raw integer.
        let parent_pid = proc.parent().map(|p| p.as_u32());

        Self {
            pid: pid.as_u32(),
            parent_pid,
            name,
            cmd,
            exe_path,
            cwd,
            cpu_usage: proc.cpu_usage(),
            memory_bytes: proc.memory(),
            status: format!("{:?}", proc.status()),
            environ_count: proc.environ().len(),
            start_time: proc.start_time(),
            run_time: proc.run_time(),
        }
    }
}
