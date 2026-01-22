//! Application entry point and runtime wiring.

use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;

mod app;
mod config;
mod events;
mod google;
mod input;
mod jobs;
mod layout;
mod shortcuts;
mod ui;
mod wizard;
mod worker;

/// Initialize file logging and keep the non-blocking guard alive.
fn init_logging() -> Result<WorkerGuard> {
    let log_file = "receipt_tui.log";
    // Write logs to a rolling file appender so the TUI stays clean.
    let file_appender = tracing_appender::rolling::never(".", log_file);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to init logging: {e}"))?;
    tracing::info!("logging to {}", log_file);
    Ok(guard)
}

#[tokio::main]
/// Entry point: init logging, start the UI, and restore the terminal.
async fn main() -> Result<()> {
    let _log_guard = init_logging()?;
    tracing::info!("app starting");
    let mut terminal = ui::init_terminal()?;
    let res = app::run_app(&mut terminal).await;
    ui::restore_terminal()?;
    if let Err(ref e) = res {
        tracing::error!("app error: {e}");
    }
    tracing::info!("app exiting");
    res
}
