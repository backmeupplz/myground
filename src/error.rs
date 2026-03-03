#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("App not found in registry: {0}")]
    NotFound(String),

    #[error("App not installed: {0}")]
    NotInstalled(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Docker compose error: {0}")]
    Compose(String),

    #[error("Backup error: {0}")]
    Backup(String),
}
