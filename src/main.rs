mod action;
mod app;
mod event;
mod process;
mod tui;
mod ui;

use std::time::Duration;

use ratatui::layout::{Constraint, Layout};
use tokio::sync::mpsc;

use crate::app::{ActiveView, App};
use crate::event::{Event, EventHandler};
use crate::process::{ProcessInfo, ProcessScanner};

/// Entry point: install hooks, init the terminal, run the app, then restore.
///
/// # Errors
///
/// Propagates any error from terminal initialisation, the main loop, or
/// terminal restoration.
#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("pid-inspector {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("pid-inspector {}", env!("CARGO_PKG_VERSION"));
        println!("A TUI process inspector for Claude Code and OpenAI Codex CLI\n");
        println!("Usage: pid-inspector\n");
        println!("Options:");
        println!("  -h, --help     Show this help message");
        println!("  -V, --version  Print version");
        return Ok(());
    }

    // Install the panic hook first so that any subsequent panic leaves the
    // terminal in a usable state and prints a formatted diagnostic.
    tui::install_panic_hook();

    let mut terminal = tui::init()?;
    run(&mut terminal).await?;
    tui::restore()?;

    Ok(())
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
async fn run(terminal: &mut tui::Tui) -> color_eyre::Result<()> {
    let mut app = App::new();
    let mut event_handler = EventHandler::new(
        Duration::from_secs(2),
        Duration::from_millis(33),
    );

    // ── Scanner channels ─────────────────────────────────────────────────────
    // scan_trigger_tx: the main loop sends a `()` to ask for a fresh scan.
    // scan_result_tx:  the blocking scanner task sends results back.
    //
    // The trigger channel is bounded(1) so a slow scanner never queues more
    // than one pending request — if the main loop ticks again before the
    // scanner finishes, try_send simply returns Err(Full) and we skip it.
    let (scan_trigger_tx, scan_trigger_rx) = mpsc::channel::<()>(1);
    let (scan_result_tx, mut scan_result_rx) = mpsc::unbounded_channel::<Vec<ProcessInfo>>();

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
                if let Some(action) = App::map_key_to_action(key, &app.active_view) {
                    app.handle_action(action);
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
                let mut latest: Option<Vec<ProcessInfo>> = None;
                while let Ok(data) = scan_result_rx.try_recv() {
                    latest = Some(data);
                }
                if let Some(data) = latest {
                    app.update_processes(data);
                }

                terminal.draw(|f| draw(f, &mut app))?;
            }

            Event::Resize => {}
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
/// # Arguments
///
/// * `trigger_rx`  - Receives `()` signals that request a new scan.
/// * `result_tx`   - Sends the resulting `Vec<ProcessInfo>` back to the main loop.
fn scanner_task(
    mut trigger_rx: mpsc::Receiver<()>,
    result_tx: mpsc::UnboundedSender<Vec<ProcessInfo>>,
) {
    // ProcessScanner::new() performs an initial seeding refresh internally,
    // so the first call to refresh() will yield meaningful CPU deltas.
    let mut scanner = ProcessScanner::new();

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

        let data = scanner.refresh();

        // If the receiver was dropped (application exiting) the error is
        // silently ignored — we just stop sending.
        if result_tx.send(data).is_err() {
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
    let [main_area, footer_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    match app.active_view {
        ActiveView::Tree => {
            ui::render_tree_view(f, main_area, &app.flat_list, &mut app.table_state);
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
                ui::render_detail_view(f, main_area, info, &cpu_hist, &mem_hist);
            }
        }
    }

    ui::render_footer(f, footer_area, &app.active_view);
}
