/// Errors that can occur during registry operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("backend already exists: {0}")]
    DuplicateBackend(String),

    #[error("backend not found: {0}")]
    BackendNotFound(String),
}
