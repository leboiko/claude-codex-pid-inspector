/// Central message type dispatched by the event loop to drive app state transitions.
///
/// Every user keystroke and timer tick is translated into one of these variants
/// before being handed to the update function, keeping input handling decoupled
/// from business logic.
#[derive(Debug)]
pub enum Action {
    /// Exit the application cleanly.
    Quit,
    /// Move the selection cursor one row up in the current list.
    MoveUp,
    /// Move the selection cursor one row down in the current list.
    MoveDown,
    /// Collapse or expand the currently selected process node.
    ToggleExpand,
    /// Enter key — navigate into the detail view for the selected process.
    SelectProcess,
    /// Escape key — return from the detail view back to the process tree.
    BackToTree,
    /// Cycle sort column forward.
    SortNext,
    /// Cycle sort column backward.
    SortPrev,
    /// Toggle sort direction (ascending / descending).
    SortToggleDirection,
    /// Request to kill the currently selected process (triggers confirmation).
    KillRequest,
    /// Confirm the pending kill.
    ConfirmKill,
    /// Cancel the pending kill.
    CancelKill,
}
