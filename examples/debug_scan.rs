use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

fn main() {
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    std::thread::sleep(std::time::Duration::from_secs(1));
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);

    println!("=== All processes with 'claude' or 'codex' in name/cmd/exe ===\n");
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_string();
        let exe = proc.exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let cmd: Vec<String> = proc.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
        let cmd_str = cmd.join(" ");

        let haystack = format!("{} {} {}", name, exe, cmd_str).to_lowercase();
        if haystack.contains("claude") || haystack.contains("codex") {
            println!("PID: {}", pid.as_u32());
            println!("  name: {:?}", name);
            println!("  exe:  {:?}", exe);
            println!("  cmd:  {:?}", cmd);
            println!("  parent: {:?}", proc.parent().map(|p| p.as_u32()));
            println!("  cpu:  {:.1}%", proc.cpu_usage());
            println!("  mem:  {} bytes", proc.memory());
            println!();
        }
    }
}
