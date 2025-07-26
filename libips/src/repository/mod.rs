//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

mod catalog;
mod file_backend;
mod rest_backend;
#[cfg(test)]
mod tests;

pub use catalog::{CatalogAttrs, CatalogManager, CatalogOperationType, CatalogPart, UpdateLog};
pub use file_backend::FileBackend;
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
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            4 => Ok(RepositoryVersion::V4),
            _ => Err(anyhow::anyhow!("Unsupported repository version: {}", value)),
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
