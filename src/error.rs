#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service not found in registry: {0}")]
    NotFound(String),

    #[error("Service already installed: {0}")]
    AlreadyInstalled(String),

    #[error("Service not installed: {0}")]
    NotInstalled(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Docker compose error: {0}")]
    Compose(String),
}
