//! Centralized `tracing` setup for every binary/crate in the workspace.
//!
//! Nothing here talks to the network: logs stay on disk under the OS-native
//! app-data directory, matching the "fully offline, nothing leaves the
//! machine" requirement for this project.

use std::path::PathBuf;

use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Where log files are written, alongside the tracker's local data.
pub fn log_dir() -> PathBuf {
    directories_next_base()
        .unwrap_or_else(std::env::temp_dir)
        .join("logs")
}

fn directories_next_base() -> Option<PathBuf> {
    // Kept intentionally dependency-light: `softpoker-config` owns the canonical
    // app-data path resolution. This is only a fallback for early startup
    // (logging initializes before config is loaded).
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/PokerTracker"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("PokerTracker"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/softpoker-tracker"))
    }
}

/// Initializes the global tracing subscriber with a console layer (level
/// controlled by `RUST_LOG`, defaulting to `level`) and a rolling file layer.
///
/// The returned [`WorkerGuard`] must be held for the lifetime of the process
/// (dropping it flushes and stops the background writer thread), so callers
/// should bind it in `main`/`run` and let it drop on shutdown.
pub fn init(default_level: LevelFilter) -> WorkerGuard {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);

    let file_appender = tracing_appender::rolling::daily(&dir, "softpoker-tracker.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::builder()
        .with_default_directive(default_level.into())
        .from_env_lossy();

    let console_layer = fmt::layer().with_target(true).with_ansi(true);
    let file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    guard
}
