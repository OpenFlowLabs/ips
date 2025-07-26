use libips::actions::ActionError;
use libips::repository;
use miette::Diagnostic;
use std::path::PathBuf;
use thiserror::Error;

/// Result type for pkg6dev operations
pub type Result<T> = std::result::Result<T, Pkg6DevError>;

/// Errors that can occur in pkg6dev operations
#[derive(Debug, Error, Diagnostic)]
pub enum Pkg6DevError {
    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::pkg6dev::io_error),
        help("Check system resources and permissions")
    )]
    IoError(#[from] std::io::Error),

    #[error("action error: {0}")]
    #[diagnostic(
        code(ips::pkg6dev::action_error),
        help("Check the action format and try again")
    )]
    ActionError(#[from] ActionError),

    #[error("repository error: {0}")]
    #[diagnostic(transparent)]
    RepositoryError(#[from] repository::RepositoryError),

    #[error("userland error: {0}")]
    #[diagnostic(
        code(ips::pkg6dev::userland_error),
        help("Check the userland component and try again")
    )]
    UserlandError(String),

    // Component-related errors
    #[error("component path error: {path} is not a valid component path")]
    #[diagnostic(
        code(ips::pkg6dev::component_path_error),
        help("Ensure the component path exists and is a directory")
    )]
    ComponentPathError {
        path: PathBuf,
    },

    #[error("manifest not found: {path}")]
    #[diagnostic(
        code(ips::pkg6dev::manifest_not_found),
        help("Ensure the manifest file exists at the specified path")
    )]
    ManifestNotFoundError {
        path: PathBuf,
    },

    #[error("replacement format error: {value} is not in the format 'key:value'")]
    #[diagnostic(
        code(ips::pkg6dev::replacement_format_error),
        help("Replacements must be in the format 'key:value'")
    )]
    ReplacementFormatError {
        value: String,
    },

    // Makefile-related errors
    #[error("makefile parse error: {message}")]
    #[diagnostic(
        code(ips::pkg6dev::makefile_parse_error),
        help("Check the Makefile syntax and try again")
    )]
    MakefileParseError {
        message: String,
    },

    #[error("component info error: {message}")]
    #[diagnostic(
        code(ips::pkg6dev::component_info_error),
        help("Check the component information and try again")
    )]
    ComponentInfoError {
        message: String,
    },

    // Package publishing errors
    #[error("manifest file not found: {path}")]
    #[diagnostic(
        code(ips::pkg6dev::manifest_file_not_found),
        help("Ensure the manifest file exists at the specified path")
    )]
    ManifestFileNotFoundError {
        path: PathBuf,
    },

    #[error("prototype directory not found: {path}")]
    #[diagnostic(
        code(ips::pkg6dev::prototype_dir_not_found),
        help("Ensure the prototype directory exists at the specified path")
    )]
    PrototypeDirNotFoundError {
        path: PathBuf,
    },

    #[error("publisher not found: {publisher}")]
    #[diagnostic(
        code(ips::pkg6dev::publisher_not_found),
        help("Add the publisher to the repository using pkg6repo add-publisher")
    )]
    PublisherNotFoundError {
        publisher: String,
    },

    #[error("no default publisher set")]
    #[diagnostic(
        code(ips::pkg6dev::no_default_publisher),
        help("Specify a publisher using the --publisher option or set a default publisher")
    )]
    NoDefaultPublisherError,

    #[error("file not found in prototype directory: {path}")]
    #[diagnostic(
        code(ips::pkg6dev::file_not_found_in_prototype),
        help("Ensure the file exists in the prototype directory")
    )]
    FileNotFoundInPrototypeError {
        path: PathBuf,
    },

    // Generic custom error (for backward compatibility)
    #[error("{0}")]
    #[diagnostic(
        code(ips::pkg6dev::custom_error),
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