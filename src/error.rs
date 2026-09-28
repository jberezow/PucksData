//! Distinguish rejected source snapshots from database failures.

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("invalid snapshot: {0}")]
    Validation(String),
}
