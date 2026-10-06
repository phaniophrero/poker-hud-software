use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine the platform app-data directory")]
    NoAppDataDir,
}

/// Root directory for local SoftPoker tracker state.
///
/// This compliant build stores only hand-history configuration, imported
/// hand-history backups, logs, and offline analysis data.
pub fn app_data_dir() -> Result<PathBuf, ConfigError> {
    directories::ProjectDirs::from("com", "softpoker", "PokerTracker")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or(ConfigError::NoAppDataDir)
}
