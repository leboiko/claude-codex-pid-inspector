use std::io::Write;
use std::time::Duration;

use clap::CommandFactory;
use clap_complete::generate;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::Block;
use tokio::sync::mpsc;

use agentop::app::{ActiveView, App, KeyContext};
use agentop::cli::{self, Cli};
use agentop::event::{Event, EventHandler};
use agentop::output::{
    build_snapshot_with_telemetry, print_table, to_compact_json, to_pretty_json,
};
use agentop::process::{
    build_forest, display_name, FlatEntryKind, ProcessInfo, ProcessScanner, SystemStats,
};
use agentop::telemetry::claude::discovery::SessionIndex;
use agentop::telemetry::codex::discovery::CodexHomeResolver;
use agentop::telemetry::{
    ClaudeTelemetryProvider, CodexTelemetryProvider, NoopProvider, TelemetryMap, TelemetryPipeline,
};
use agentop::{tui, ui};

/// Entry point: parse arguments, then dispatch to the appropriate mode.
///
/// # Errors
///
/// Propagates any error from terminal initialisation, the main loop, or
/// terminal restoration.
#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let args = cli::parse();

    // Shell completions: print and exit before touching the terminal.
    if let Some(shell) = args.generate_completions {
        let mut cmd = Cli::command();
        generate(
            clap_complete::Shell::from(shell),
            &mut cmd,
            "agentop",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    // Non-interactive output modes share the two-sample CPU warm-up logic.
    if args.list || args.json {
        run_one_shot(&args)?;
        return Ok(());
    }

    // Default: interactive TUI.
    tui::install_panic_hook();
    let mut terminal = tui::init()?;
    run_tui(&mut terminal).await?;
    tui::restore()?;

    Ok(())
}

/// Perform a two-sample scan, then print the requested non-interactive output.
///
/// sysinfo CPU deltas require two refresh calls separated by a short interval;
/// without this, all CPU readings are 0.0 on the first (and only) call.
///
/// # Errors
///
/// Returns an error if JSON serialisation or stdout I/O fails.
fn run_one_shot(args: &Cli) -> color_eyre::Result<()> {
    let mut scanner = ProcessScanner::new();
    // First refresh seeds the CPU counters.
    let _ = scanner.refresh();
    // Brief pause so the second delta is meaningful.
    std::thread::sleep(Duration::from_millis(500));
    let (processes, stats) = scanner.refresh();

    // Mirror the TUI's provider selection: enable Claude and Codex providers
    // when their on-disk state directories exist. Fall back to NoopProvider on
    // machines with neither installed.
    let mut pipeline = {
        let mut p = TelemetryPipeline::new();
        let claude_probe = SessionIndex::new();
        if claude_probe.sessions_dir_exists() {
            p = p.with_provider(Box::new(ClaudeTelemetryProvider::new()));
        }
        let codex_probe = CodexHomeResolver::new();
        if codex_probe.default_sessions_dir_exists() {
            p = p.with_provider(Box::new(CodexTelemetryProvider::new()));
        }
        if p.provider_names().is_empty() {
            p = p.with_provider(Box::new(NoopProvider));
        }
        p
    };
    let telemetry = pipeline.enrich(&processes);

    let mut forest = build_forest(&processes);

    // Compute agent summary from the forest (mirrors App::compute_agent_summary).
    let summary = compute_summary_from_forest(&forest);

    // Sort roots by CPU descending for a sensible default order.
    forest.sort_by(|a, b| {
        b.subtree_stats
            .total_cpu
            .partial_cmp(&a.subtree_stats.total_cpu)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if args.list {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        print_table(&forest, &mut lock)?;
        return Ok(());
    }

    if args.json {
        // Activity states need the App state machine to be tracked, so in
        // one-shot mode they are absent. Telemetry is still available via
        // the provider pipeline above.
        let snapshot = build_snapshot_with_telemetry(
            &forest,
            &stats,
            &summary,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            &telemetry,
        );
        let json = if args.pretty {
            to_pretty_json(&snapshot)?
        } else {
            to_compact_json(&snapshot)?
        };
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        writeln!(lock, "{json}")?;
        return Ok(());
    }

    Ok(())
}

/// Compute an [`agentop::app::AgentSummary`] from the forest without needing
/// a full [`App`] instance.
fn compute_summary_from_forest(
    forest: &[agentop::process::ProcessNode],
) -> agentop::app::AgentSummary {
    use agentop::app::AgentSummary;
    use agentop::process::process_kind;
    use agentop::process::ProcessKind;

    let mut summary = AgentSummary::default();
    for root in forest {
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

/// Application loop: drives events, scanner, state updates, and rendering.
///
/// Exits when [`App::should_quit`] is set to `true`.
///
/// # Arguments
///
/// * `terminal` - Mutable reference to the crossterm-backed ratatui terminal.
///
/// # Errors
///
/// Returns an error if the event channel closes unexpectedly or if ratatui
/// fails to draw a frame.
async fn run_tui(terminal: &mut tui::Tui) -> color_eyre::Result<()> {
    let mut app = App::new();
    let mut event_handler = EventHandler::new(Duration::from_secs(2), Duration::from_millis(33));

    // ── Scanner channels ─────────────────────────────────────────────────────
    // scan_trigger_tx: the main loop sends a `()` to ask for a fresh scan.
    // scan_result_tx:  the blocking scanner task sends results back.
    //
    // The trigger channel is bounded(1) so a slow scanner never queues more
    // than one pending request — if the main loop ticks again before the
    // scanner finishes, try_send simply returns Err(Full) and we skip it.
    let (scan_trigger_tx, scan_trigger_rx) = mpsc::channel::<()>(1);
    let (scan_result_tx, mut scan_result_rx) =
        mpsc::unbounded_channel::<(Vec<ProcessInfo>, SystemStats, TelemetryMap)>();

    // Spawn the scanner on a blocking thread pool thread so the `sysinfo`
    // syscalls never block the async reactor.
    tokio::task::spawn_blocking(move || {
        scanner_task(scan_trigger_rx, scan_result_tx);
    });

    // Prime the pump: request an immediate scan so data appears on the first
    // render rather than after the first 2-second tick.
    //
    // Unwrap is intentional here: if this send fails the scanner task has
    // already panicked, which is a programming error we want to surface.
    scan_trigger_tx
        .try_send(())
        .expect("initial scan trigger failed");

    // ── Main event loop ───────────────────────────────────────────────────────
    loop {
        match event_handler.next().await? {
            Event::Key(key) => {
                let was_help_open = app.help_open;
                let ctx = KeyContext {
                    active_view: &app.active_view,
                    confirming_kill: app.confirm_kill_pid.is_some(),
                    config_open: app.config_popup.is_some(),
                    filter_active: app.filter_active,
                    help_open: app.help_open,
                };
                if let Some(action) = App::map_key_to_action(key, &ctx) {
                    app.handle_action(action);
                    // When the help overlay was open and is now closed by a
                    // non-Esc key, re-dispatch the same key so the intended
                    // action executes (spec: "any other bound key closes AND
                    // executes the action").
                    if was_help_open && !app.help_open {
                        let ctx2 = KeyContext {
                            active_view: &app.active_view,
                            confirming_kill: app.confirm_kill_pid.is_some(),
                            config_open: app.config_popup.is_some(),
                            filter_active: app.filter_active,
                            help_open: false,
                        };
                        if let Some(action2) = App::map_key_to_action(key, &ctx2) {
                            // Skip re-toggling help to avoid re-opening it.
                            if !matches!(action2, agentop::action::Action::ToggleHelp) {
                                app.handle_action(action2);
                            }
                        }
                    }
                }
            }

            Event::Tick => {
                // Non-blocking: if the scanner is still busy with the previous
                // request, the channel is full and we simply skip this tick.
                let _ = scan_trigger_tx.try_send(());
            }

            Event::Render => {
                // Drain all available scan results. In practice there will be
                // at most one, but draining keeps the channel from backing up
                // if renders are skipped or the scanner delivers early.
                let mut latest: Option<(Vec<ProcessInfo>, SystemStats, TelemetryMap)> = None;
                while let Ok(data) = scan_result_rx.try_recv() {
                    latest = Some(data);
                }
                if let Some((procs, stats, telemetry)) = latest {
                    app.update_processes(procs, stats, telemetry);
                }

                terminal.draw(|f| draw(f, &mut app))?;
            }

            Event::Resize => {
                terminal.draw(|f| draw(f, &mut app))?;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Blocking scanner task: waits for trigger signals and sends results back.
///
/// Runs for the lifetime of the application on a dedicated blocking thread.
/// Using `Handle::current().block_on` lets us await async channel operations
/// from within a `spawn_blocking` context — the async runtime is still active,
/// but this thread is allowed to block.
///
/// The telemetry pipeline is constructed here and lives on this thread for
/// its entire lifetime — no locks or `Arc` wrappers are needed because the
/// pipeline is accessed from only this single thread.
///
/// # Arguments
///
/// * `trigger_rx`  - Receives `()` signals that request a new scan.
/// * `result_tx`   - Sends `(processes, stats, telemetry)` back to the main loop.
fn scanner_task(
    mut trigger_rx: mpsc::Receiver<()>,
    result_tx: mpsc::UnboundedSender<(Vec<ProcessInfo>, SystemStats, TelemetryMap)>,
) {
    // ProcessScanner::new() performs an initial seeding refresh internally,
    // so the first call to refresh() will yield meaningful CPU deltas.
    let mut scanner = ProcessScanner::new();

    // Build the telemetry pipeline. Register Claude and Codex providers when
    // their on-disk state directories exist. Fall back to NoopProvider on
    // machines with neither installed.
    let mut pipeline = {
        let mut p = TelemetryPipeline::new();
        let claude_probe = SessionIndex::new();
        if claude_probe.sessions_dir_exists() {
            p = p.with_provider(Box::new(ClaudeTelemetryProvider::new()));
        }
        let codex_probe = CodexHomeResolver::new();
        if codex_probe.default_sessions_dir_exists() {
            p = p.with_provider(Box::new(CodexTelemetryProvider::new()));
        }
        if p.provider_names().is_empty() {
            p = p.with_provider(Box::new(NoopProvider));
        }
        p
    };

    // Grab the handle to the current Tokio runtime so we can block on async
    // channel receives from this synchronous context.
    let handle = tokio::runtime::Handle::current();

    loop {
        // Block this thread until a trigger arrives (or the channel closes).
        let received = handle.block_on(trigger_rx.recv());

        // `None` means all senders were dropped — the main loop exited.
        if received.is_none() {
            break;
        }

        let (processes, stats) = scanner.refresh();
        let telemetry = pipeline.enrich(&processes);

        // If the receiver was dropped (application exiting) the error is
        // silently ignored — we just stop sending.
        if result_tx.send((processes, stats, telemetry)).is_err() {
            break;
        }
    }
}

/// Draw a single frame: split into main content and a one-line footer.
///
/// Dispatches to the correct view renderer based on [`App::active_view`].
///
/// # Arguments
///
/// * `f`   - Ratatui frame for this render cycle.
/// * `app` - Mutable application state (table selection state needs `&mut`).
fn draw(f: &mut ratatui::Frame, app: &mut App) {
    // Expire any transient status message that has timed out.
    app.tick_status_message();

    let [status_area, main_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let palette = &app.palette;

    // Fill the entire terminal with the theme's background color before any
    // widgets render. This ensures light themes actually paint the background
    // instead of leaving the terminal's default dark color showing through.
    f.render_widget(Block::default().style(palette.base_style()), f.area());

    let visible_count = app.visible_process_count();
    let total_count = app.total_process_count();
    let status_msg = app.status_message.as_ref().map(|(s, _)| s.as_str());
    ui::render_status_bar(
        f,
        status_area,
        &app.system_stats,
        &app.agent_summary,
        app.focus_filter,
        &app.filter_text,
        visible_count,
        total_count,
        status_msg,
    );

    // Detect terminal adapter once per frame for footer hint.
    let terminal_jump_enabled = agentop::terminals::detect_from_env().is_some();

    match app.active_view {
        ActiveView::Tree => {
            ui::render_tree_view(
                f,
                main_area,
                &app.flat_list,
                &mut app.table_state,
                app.sort_column,
                app.sort_direction,
                palette,
                &app.telemetry,
                app.telemetry_view,
                &app.cost_burn_history,
            );
        }
        ActiveView::Detail => {
            if let Some(ref info) = app.selected_detail {
                // Collect history ring-buffers into plain `Vec`s for the renderer.
                let cpu_hist: Vec<f32> = app
                    .cpu_history
                    .get(&info.pid)
                    .map(|d| d.iter().copied().collect())
                    .unwrap_or_default();
                let mem_hist: Vec<u64> = app
                    .mem_history
                    .get(&info.pid)
                    .map(|d| d.iter().copied().collect())
                    .unwrap_or_default();
                let tel = app
                    .selected_detail
                    .as_ref()
                    .and_then(|d| app.telemetry.get(&d.pid));
                ui::render_detail_view(
                    f,
                    main_area,
                    info,
                    &cpu_hist,
                    &mem_hist,
                    app.selected_detail_subtree,
                    app.graph_style,
                    palette,
                    tel,
                );
            }
        }
    }

    ui::render_footer(
        f,
        footer_area,
        &app.active_view,
        palette,
        app.filter_active,
        &app.filter_text,
        terminal_jump_enabled,
    );

    // Popups render on top of everything else. Help overlay goes on top of all other
    // popups. Config popup takes precedence over kill popups.
    if app.help_open {
        ui::render_help_overlay(f, palette);
    } else if let Some(ref config_state) = app.config_popup {
        ui::render_config_popup(f, config_state, app.graph_style, app.theme, palette);
    } else if let Some(pid) = app.confirm_kill_pid {
        let name = app
            .flat_list
            .iter()
            .find(|e| matches!(e.row_kind, FlatEntryKind::Process) && e.info.pid == pid)
            .map(|e| display_name(&e.info))
            .unwrap_or("unknown");
        ui::render_kill_confirm(f, pid, name, palette);
    } else if let Some(ref msg) = app.kill_result {
        ui::render_kill_result(f, msg, palette);
    }
}
