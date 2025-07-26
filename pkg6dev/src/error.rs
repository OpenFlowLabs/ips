use libips::actions::ActionError;
use libips::repository;
use miette::Diagnostic;
use thiserror::Error;

/// Result type for pkg6dev operations
pub type Result<T> = std::result::Result<T, Pkg6DevError>;

/// Errors that can occur in pkg6dev operations
#[derive(Debug, Error, Diagnostic)]
pub enum Pkg6DevError {
    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::pkg6dev_error::io),
        help("Check system resources and permissions")
    )]
    IoError(#[from] std::io::Error),

    #[error("action error: {0}")]
    #[diagnostic(
        code(ips::pkg6dev_error::action),
        help("Check the action format and try again")
    )]
    ActionError(#[from] ActionError),

    #[error("repository error: {0}")]
    #[diagnostic(transparent)]
    RepositoryError(#[from] repository::RepositoryError),

    #[error("userland error: {0}")]
    #[diagnostic(
        code(ips::pkg6dev_error::userland),
        help("Check the userland component and try again")
    )]
    UserlandError(String),

    #[error("{0}")]
    #[diagnostic(
        code(ips::pkg6dev_error::custom),
        help("See error message for details")
    )]
    Custom(String),
}

/// Convert a string to a Pkg6DevError::Custom
impl From<String> for Pkg6DevError {
    fn from(s: String) -> Self {
        Pkg6DevError::Custom(s)
    }
}

/// Convert a &str to a Pkg6DevError::Custom
impl From<&str> for Pkg6DevError {
    fn from(s: &str) -> Self {
        Pkg6DevError::Custom(s.to_string())
    }
}

/// Convert an anyhow::Error to a Pkg6DevError::UserlandError
impl From<anyhow::Error> for Pkg6DevError {
    fn from(e: anyhow::Error) -> Self {
        Pkg6DevError::UserlandError(e.to_string())
    }
}