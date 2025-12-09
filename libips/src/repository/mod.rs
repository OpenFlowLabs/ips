//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use miette::Diagnostic;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf, StripPrefixError};
use thiserror::Error;

/// Result type for repository operations
pub type Result<T> = std::result::Result<T, RepositoryError>;

/// Errors that can occur in repository operations
#[derive(Debug, Error, Diagnostic)]
pub enum RepositoryError {
    #[error("unsupported repository version: {0}")]
    #[diagnostic(
        code(ips::repository_error::unsupported_version),
        help("Supported repository versions: 4")
    )]
    UnsupportedVersion(u32),

    #[error("repository not found at {0}")]
    #[diagnostic(
        code(ips::repository_error::not_found),
        help("Check that the repository path exists and is accessible")
    )]
    NotFound(String),

    #[error("publisher {0} not found")]
    #[diagnostic(
        code(ips::repository_error::publisher_not_found),
        help("Check that the publisher name is correct and exists in the repository")
    )]
    PublisherNotFound(String),

    #[error("publisher {0} already exists")]
    #[diagnostic(
        code(ips::repository_error::publisher_exists),
        help("Use a different publisher name or remove the existing publisher first")
    )]
    PublisherExists(String),

    #[error("failed to read repository configuration: {0}")]
    #[diagnostic(
        code(ips::repository_error::config_read),
        help("Check that the repository configuration file exists and is valid")
    )]
    ConfigReadError(String),

    #[error("failed to write repository configuration: {0}")]
    #[diagnostic(
        code(ips::repository_error::config_write),
        help("Check that the repository directory is writable")
    )]
    ConfigWriteError(String),

    #[error("failed to create directory {path}: {source}")]
    #[diagnostic(
        code(ips::repository_error::directory_create),
        help("Check that the parent directory exists and is writable")
    )]
    DirectoryCreateError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to read file {path}: {source}")]
    #[diagnostic(
        code(ips::repository_error::file_read),
        help("Check that the file exists and is readable")
    )]
    FileReadError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to write file {path}: {source}")]
    #[diagnostic(
        code(ips::repository_error::file_write),
        help("Check that the directory is writable")
    )]
    FileWriteError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to parse JSON: {0}")]
    #[diagnostic(
        code(ips::repository_error::json_parse),
        help("Check that the JSON file is valid")
    )]
    JsonParseError(String),

    #[error("failed to serialize JSON: {0}")]
    #[diagnostic(
        code(ips::repository_error::json_serialize),
        help("This is likely a bug in the code")
    )]
    JsonSerializeError(String),

    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::repository_error::io),
        help("Check system resources and permissions")
    )]
    IoError(#[from] io::Error),

    #[error("other error: {0}")]
    #[diagnostic(
        code(ips::repository_error::other),
        help("See error message for details")
    )]
    Other(String),

    #[error("JSON error: {0}")]
    #[diagnostic(
        code(ips::repository_error::json_error),
        help("Check the JSON format and try again")
    )]
    JsonError(String),

    #[error("digest error: {0}")]
    #[diagnostic(
        code(ips::repository_error::digest_error),
        help("Check the digest format and try again")
    )]
    DigestError(String),

    #[error(transparent)]
    #[diagnostic(transparent)]
    ActionError(#[from] ActionError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    CatalogError(#[from] catalog::CatalogError),

    #[error("path prefix error: {0}")]
    #[diagnostic(
        code(ips::repository_error::path_prefix),
        help("Check that the path is valid and within the expected directory")
    )]
    PathPrefixError(String),
}

// Implement From for common error types
impl From<serde_json::Error> for RepositoryError {
    fn from(err: serde_json::Error) -> Self {
        RepositoryError::JsonError(err.to_string())
    }
}

impl From<DigestError> for RepositoryError {
    fn from(err: DigestError) -> Self {
        RepositoryError::DigestError(err.to_string())
    }
}

impl From<StripPrefixError> for RepositoryError {
    fn from(err: StripPrefixError) -> Self {
        RepositoryError::PathPrefixError(err.to_string())
    }
}

// Implement From for redb error types
impl From<redb::Error> for RepositoryError {
    fn from(err: redb::Error) -> Self {
        RepositoryError::Other(format!("Database error: {}", err))
    }
}

impl From<redb::DatabaseError> for RepositoryError {
    fn from(err: redb::DatabaseError) -> Self {
        RepositoryError::Other(format!("Database error: {}", err))
    }
}

impl From<redb::TransactionError> for RepositoryError {
    fn from(err: redb::TransactionError) -> Self {
        RepositoryError::Other(format!("Transaction error: {}", err))
    }
}

impl From<redb::TableError> for RepositoryError {
    fn from(err: redb::TableError) -> Self {
        RepositoryError::Other(format!("Table error: {}", err))
    }
}

impl From<redb::StorageError> for RepositoryError {
    fn from(err: redb::StorageError) -> Self {
        RepositoryError::Other(format!("Storage error: {}", err))
    }
}

impl From<redb::CommitError> for RepositoryError {
    fn from(err: redb::CommitError) -> Self {
        RepositoryError::Other(format!("Commit error: {}", err))
    }
}

impl From<bincode::error::DecodeError> for RepositoryError {
    fn from(err: bincode::error::DecodeError) -> Self {
        RepositoryError::Other(format!("Serialization error: {}", err))
    }
}

impl From<bincode::error::EncodeError> for RepositoryError {
    fn from(err: bincode::error::EncodeError) -> Self {
        RepositoryError::Other(format!("Serialization error: {}", err))
    }
}
pub mod catalog;
pub(crate) mod file_backend;
mod catalog_writer;
mod obsoleted;
pub mod progress;
mod rest_backend;
#[cfg(test)]
mod tests;

use crate::actions::ActionError;
use crate::digest::DigestError;
pub use catalog::{
    CatalogAttrs, CatalogError, CatalogManager, CatalogOperationType, CatalogPart, UpdateLog,
};
pub use file_backend::FileBackend;
pub use obsoleted::{ObsoletedPackageManager, ObsoletedPackageMetadata};
pub use progress::{ProgressInfo, ProgressReporter, NoopProgressReporter};
pub use rest_backend::RestBackend;

/// Repository configuration filename
pub const REPOSITORY_CONFIG_FILENAME: &str = "pkg6.repository";

/// Information about a publisher in a repository
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PublisherInfo {
    /// Name of the publisher
    pub name: String,
    /// Number of packages from this publisher
    pub package_count: usize,
    /// Status of the publisher (e.g., "online", "offline")
    pub status: String,
    /// Last updated timestamp in ISO 8601 format
    pub updated: String,
}

/// Information about a repository
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepositoryInfo {
    /// Information about publishers in the repository
    pub publishers: Vec<PublisherInfo>,
}

/// Information about a package in a repository
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageInfo {
    /// FMRI (Fault Management Resource Identifier) of the package
    pub fmri: crate::fmri::Fmri,
}

/// Contents of a package
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageContents {
    /// Package identifier (name and version)
    pub package_id: String,
    /// Files in the package
    pub files: Option<Vec<String>>,
    /// Directories in the package
    pub directories: Option<Vec<String>>,
    /// Links in the package
    pub links: Option<Vec<String>>,
    /// Dependencies of the package
    pub dependencies: Option<Vec<String>>,
    /// Licenses in the package
    pub licenses: Option<Vec<String>>,
}

/// Repository version
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RepositoryVersion {
    V4 = 4,
}

impl Default for RepositoryVersion {
    fn default() -> Self {
        RepositoryVersion::V4
    }
}

impl std::convert::TryFrom<u32> for RepositoryVersion {
    type Error = RepositoryError;

    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        match value {
            4 => Ok(RepositoryVersion::V4),
            _ => Err(RepositoryError::UnsupportedVersion(value)),
        }
    }
}

/// Repository configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepositoryConfig {
    pub version: RepositoryVersion,
    pub publishers: Vec<String>,
    pub properties: HashMap<String, String>,
    pub default_publisher: Option<String>,
}

impl Default for RepositoryConfig {
    fn default() -> Self {
        RepositoryConfig {
            version: RepositoryVersion::default(),
            publishers: Vec::new(),
            properties: HashMap::new(),
            default_publisher: None,
        }
    }
}

/// Repository trait for read-only operations
pub trait ReadableRepository {
    /// Open an existing repository
    fn open<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        Self: Sized;

    /// Get repository information
    fn get_info(&self) -> Result<RepositoryInfo>;

    /// List packages in the repository
    fn list_packages(
        &self,
        publisher: Option<&str>,
        pattern: Option<&str>,
    ) -> Result<Vec<PackageInfo>>;

    /// Show contents of packages
    fn show_contents(
        &self,
        publisher: Option<&str>,
        pattern: Option<&str>,
        action_types: Option<&[String]>,
    ) -> Result<Vec<PackageContents>>;

    /// Fetch a content payload identified by digest into the destination path.
    /// Implementations should download/copy the payload to a temporary path,
    /// verify integrity, and atomically move into `dest`.
    fn fetch_payload(
        &mut self,
        publisher: &str,
        digest: &str,
        dest: &Path,
    ) -> Result<()>;

    /// Fetch a package manifest by FMRI from the repository.
    /// Implementations should retrieve and parse the manifest for the given
    /// publisher and fully-qualified FMRI (name@version).
    fn fetch_manifest(
        &mut self,
        publisher: &str,
        fmri: &crate::fmri::Fmri,
    ) -> Result<crate::actions::Manifest>;

    /// Search for packages in the repository
    ///
    /// This method searches for packages in the repository using the search index.
    /// It returns a list of packages that match the search query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query
    /// * `publisher` - Optional publisher to limit the search to
    /// * `limit` - Optional maximum number of results to return
    fn search(
        &self,
        query: &str,
        publisher: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<PackageInfo>>;
}

/// Repository trait for write operations
pub trait WritableRepository {
    /// Create a new repository at the specified path
    fn create<P: AsRef<Path>>(path: P, version: RepositoryVersion) -> Result<Self>
    where
        Self: Sized;

    /// Save the repository configuration
    fn save_config(&self) -> Result<()>;

    /// Add a publisher to the repository
    fn add_publisher(&mut self, publisher: &str) -> Result<()>;

    /// Remove a publisher from the repository
    fn remove_publisher(&mut self, publisher: &str, dry_run: bool) -> Result<()>;

    /// Set a repository property
    fn set_property(&mut self, property: &str, value: &str) -> Result<()>;

    /// Set a publisher property
    fn set_publisher_property(
        &mut self,
        publisher: &str,
        property: &str,
        value: &str,
    ) -> Result<()>;

    /// Rebuild repository metadata
    fn rebuild(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()>;

    /// Refresh repository metadata
    fn refresh(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()>;

    /// Set the default publisher for the repository
    fn set_default_publisher(&mut self, publisher: &str) -> Result<()>;
}

/// Repository trait defining the interface for all repository backends
///
/// This trait combines both ReadableRepository and WritableRepository traits
/// for backward compatibility.
pub trait Repository: ReadableRepository + WritableRepository {}
