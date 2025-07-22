//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use std::path::Path;
use std::collections::HashMap;

mod file_backend;
mod rest_backend;

pub use file_backend::FileBackend;
pub use rest_backend::RestBackend;

/// Repository configuration filename
pub const REPOSITORY_CONFIG_FILENAME: &str = "pkg6.repository";

/// Information about a publisher in a repository
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInfo {
    /// Information about publishers in the repository
    pub publishers: Vec<PublisherInfo>,
}

/// Information about a package in a repository
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    /// FMRI (Fault Management Resource Identifier) of the package
    pub fmri: crate::fmri::Fmri,
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

/// Repository trait defining the interface for all repository backends
pub trait Repository {
    /// Create a new repository at the specified path
    fn create<P: AsRef<Path>>(path: P, version: RepositoryVersion) -> Result<Self> where Self: Sized;
    
    /// Open an existing repository
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> where Self: Sized;
    
    /// Save the repository configuration
    fn save_config(&self) -> Result<()>;
    
    /// Add a publisher to the repository
    fn add_publisher(&mut self, publisher: &str) -> Result<()>;
    
    /// Remove a publisher from the repository
    fn remove_publisher(&mut self, publisher: &str, dry_run: bool) -> Result<()>;
    
    /// Get repository information
    fn get_info(&self) -> Result<RepositoryInfo>;
    
    /// Set a repository property
    fn set_property(&mut self, property: &str, value: &str) -> Result<()>;
    
    /// Set a publisher property
    fn set_publisher_property(&mut self, publisher: &str, property: &str, value: &str) -> Result<()>;
    
    /// List packages in the repository
    fn list_packages(&self, publisher: Option<&str>, pattern: Option<&str>) -> Result<Vec<PackageInfo>>;
    
    /// Show contents of packages
    fn show_contents(&self, publisher: Option<&str>, pattern: Option<&str>, action_types: Option<&[String]>) -> Result<Vec<(String, String, String)>>;
    
    /// Rebuild repository metadata
    fn rebuild(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()>;
    
    /// Refresh repository metadata
    fn refresh(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()>;
    
    /// Set the default publisher for the repository
    fn set_default_publisher(&mut self, publisher: &str) -> Result<()>;
}