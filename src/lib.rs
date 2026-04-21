//! Library interface for `agentop`.
//!
//! Exposes the core modules so integration tests in `tests/` and future
//! embedding use cases can access types without going through the binary
//! entry point.
//!
//! The application binary at `src/main.rs` uses this crate as its own
//! dependency via the implicit Cargo lib+bin layout.

pub mod action;
pub mod app;
pub mod cli;
pub mod config;
pub mod event;
pub mod output;
pub mod process;
pub mod telemetry;
pub mod tui;
pub mod ui;
