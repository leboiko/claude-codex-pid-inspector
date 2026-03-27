use std::io::{stdout, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::prelude::*;

/// A [`Terminal`] backed by the process's standard output.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Switch the terminal into raw/alternate-screen mode and return a [`Tui`].
///
/// Call [`restore`] (or rely on [`install_panic_hook`]) to undo these changes
/// before the process exits.
///
/// # Errors
///
/// Propagates any I/O error from crossterm or ratatui initialisation.
pub fn init() -> color_eyre::Result<Tui> {
    enable_raw_mode()?;
    // EnterAlternateScreen hides the scrollback buffer; EnableMouseCapture lets
    // the app receive mouse events from crossterm.
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    Ok(terminal)
}

/// Restore the terminal to its original state.
///
/// Should be called on clean exit **and** inside the panic/error hooks so that
/// the terminal is left usable regardless of how the process terminates.
///
/// # Errors
///
/// Propagates any I/O error from crossterm.
pub fn restore() -> color_eyre::Result<()> {
    execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    disable_raw_mode()?;
    Ok(())
}

/// Install panic and color_eyre error hooks that call [`restore`] before
/// printing diagnostics, ensuring the terminal is never left in raw mode.
pub fn install_panic_hook() {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    // Install the eyre hook first so that `color_eyre::install()` is satisfied
    // before we set the panic hook (which may itself trigger eyre internals).
    eyre_hook.install().expect("failed to install eyre hook");

    // Wrap the default panic hook: restore the terminal, then delegate to the
    // original formatter so the panic message is still visible.
    let panic_hook = panic_hook.into_panic_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        panic_hook(info);
    }));
}
