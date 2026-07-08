use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("provider not configured: {0}")]
    ProviderNotConfigured(String),
    #[error("external send not allowed: user consent required")]
    ConsentRequired,
    #[error("secret store error: {0}")]
    SecretStore(String),
    #[error("indexing error: {0}")]
    Indexing(String),
}

pub type DomainResult<T> = Result<T, DomainError>;
