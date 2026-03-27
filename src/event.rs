use std::time::Duration;

use color_eyre::eyre::eyre;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEvent, KeyEventKind};
use futures::{FutureExt, StreamExt};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time,
};

/// Events emitted by the [`EventHandler`] to drive the application loop.
#[derive(Debug)]
pub enum Event {
    /// A key was pressed.
    Key(KeyEvent),
    /// A periodic tick for logic/state updates.
    Tick,
    /// A periodic signal to redraw the UI.
    Render,
    /// The terminal was resized.
    Resize,
}

/// Async event handler that multiplexes crossterm input, tick, and render events
/// over a single unbounded channel.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    // Keep the task alive for the lifetime of the handler.
    _task: JoinHandle<()>,
}

impl EventHandler {
    /// Spawn the background event loop and return a connected handler.
    ///
    /// # Arguments
    ///
    /// * `tick_rate`   - Interval between [`Event::Tick`] emissions.
    /// * `render_rate` - Interval between [`Event::Render`] emissions.
    pub fn new(tick_rate: Duration, render_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let _task = tokio::spawn(event_loop(tx, tick_rate, render_rate));

        Self { rx, _task }
    }

    /// Await the next event from the channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the sender side has been dropped (task panicked or
    /// was cancelled).
    pub async fn next(&mut self) -> color_eyre::Result<Event> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| eyre!("event channel closed"))
    }
}

/// Background task: select over tick timer, render timer, and crossterm events.
async fn event_loop(
    tx: mpsc::UnboundedSender<Event>,
    tick_rate: Duration,
    render_rate: Duration,
) {
    let mut tick_interval = time::interval(tick_rate);
    let mut render_interval = time::interval(render_rate);
    let mut reader = EventStream::new();

    loop {
        // `.fuse()` prevents polling a completed future after it resolves.
        let crossterm_event = reader.next().fuse();

        tokio::select! {
            _ = tick_interval.tick() => {
                if tx.send(Event::Tick).is_err() { break; }
            }
            _ = render_interval.tick() => {
                if tx.send(Event::Render).is_err() { break; }
            }
            maybe_event = crossterm_event => {
                match maybe_event {
                    Some(Ok(CrosstermEvent::Key(key))) => {
                        // Only forward actual key-press events; ignore release/repeat.
                        if key.kind == KeyEventKind::Press
                            && tx.send(Event::Key(key)).is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(CrosstermEvent::Resize(_, _))) => {
                        if tx.send(Event::Resize).is_err() { break; }
                    }
                    // Ignore mouse events, focus events, paste events, etc.
                    Some(Ok(_)) => {}
                    // Stream ended or I/O error — terminate the task.
                    Some(Err(_)) | None => break,
                }
            }
        }
    }
}
