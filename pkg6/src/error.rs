use libips::fmri::FmriError;
use libips::image::ImageError;
use miette::Diagnostic;
use thiserror::Error;

/// Result type for pkg6 operations
pub type Result<T> = std::result::Result<T, Pkg6Error>;

/// Errors that can occur in pkg6 operations
#[derive(Debug, Error, Diagnostic)]
pub enum Pkg6Error {
    #[error("I/O error: {0}")]
    #[diagnostic(
        code(pkg6::io_error),
        help("Check system resources and permissions")
    )]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    #[diagnostic(
        code(pkg6::json_error),
        help("Check the JSON format and try again")
    )]
    JsonError(#[from] serde_json::Error),

    #[error("FMRI error: {0}")]
    #[diagnostic(
        code(pkg6::fmri_error),
        help("Check the package FMRI format and try again")
    )]
    FmriError(#[from] FmriError),

    #[error("Image error: {0}")]
    #[diagnostic(
        code(pkg6::image_error),
        help("Check the image configuration and try again")
    )]
    ImageError(#[from] ImageError),

    #[error("logging environment setup error: {0}")]
    #[diagnostic(
        code(pkg6::logging_env_error),
        help("Check your logging environment configuration and try again")
    )]
    LoggingEnvError(String),

    #[error("unsupported output format: {0}")]
    #[diagnostic(
        code(pkg6::unsupported_output_format),
        help("Supported output formats: table, json, tsv")
    )]
    UnsupportedOutputFormat(String),

    #[error("other error: {0}")]
    #[diagnostic(code(pkg6::other_error), help("See error message for details"))]
    Other(String),
}

/// Convert a string to a Pkg6Error::Other
impl From<String> for Pkg6Error {
    fn from(s: String) -> Self {
        Pkg6Error::Other(s)
    }
}

/// Convert a &str to a Pkg6Error::Other
impl From<&str> for Pkg6Error {
    fn from(s: &str) -> Self {
        Pkg6Error::Other(s.to_string())
    }
}