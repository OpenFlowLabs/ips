//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;

use crate::fmri::Fmri;

/// Errors that can occur in catalog operations
#[derive(Debug, Error, Diagnostic)]
pub enum CatalogError {
    #[error("catalog part does not exist: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::part_not_found),
        help("Check that the catalog part exists and is accessible")
    )]
    CatalogPartNotFound {
        name: String,
    },

    #[error("catalog part not loaded: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::part_not_loaded),
        help("Load the catalog part before attempting to save it")
    )]
    CatalogPartNotLoaded {
        name: String,
    },

    #[error("update log not loaded: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::update_log_not_loaded),
        help("Load the update log before attempting to save it")
    )]
    UpdateLogNotLoaded {
        name: String,
    },

    #[error("update log does not exist: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::update_log_not_found),
        help("Check that the update log exists and is accessible")
    )]
    UpdateLogNotFound {
        name: String,
    },

    #[error("failed to serialize JSON: {0}")]
    #[diagnostic(
        code(ips::repository_error::catalog::json_serialize),
        help("This is likely a bug in the code")
    )]
    JsonSerializationError(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::repository_error::catalog::io),
        help("Check system resources and permissions")
    )]
    IoError(#[from] io::Error),
}

/// Result type for catalog operations
pub type Result<T> = std::result::Result<T, CatalogError>;

/// Format a SystemTime as an ISO-8601 'basic format' date in UTC
fn format_iso8601_basic(time: &SystemTime) -> String {
    let datetime = convert_system_time_to_datetime(time);
    format!("{}Z", datetime.format("%Y%m%dT%H%M%S.%f"))
}

/// Convert SystemTime to UTC DateTime, handling errors gracefully
fn convert_system_time_to_datetime(time: &SystemTime) -> chrono::DateTime<chrono::Utc> {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));

    let secs = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos();

    chrono::DateTime::from_timestamp(secs, nanos).unwrap_or_else(|| {
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            chrono::NaiveDateTime::default(),
            chrono::Utc,
        )
    })
}

/// Catalog version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogVersion {
    V1 = 1,
}

impl Default for CatalogVersion {
    fn default() -> Self {
        CatalogVersion::V1
    }
}

/// Catalog part information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPartInfo {
    /// Last modified timestamp in ISO-8601 'basic format' date in UTC
    #[serde(rename = "last-modified")]
    pub last_modified: String,

    /// Optional SHA-1 signature of the catalog part
    #[serde(rename = "signature-sha-1", skip_serializing_if = "Option::is_none")]
    pub signature_sha1: Option<String>,
}

/// Update log information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateLogInfo {
    /// Last modified timestamp in ISO-8601 'basic format' date in UTC
    #[serde(rename = "last-modified")]
    pub last_modified: String,

    /// Optional SHA-1 signature of the update log
    #[serde(rename = "signature-sha-1", skip_serializing_if = "Option::is_none")]
    pub signature_sha1: Option<String>,
}

/// Catalog attributes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogAttrs {
    /// Optional signature information
    #[serde(rename = "_SIGNATURE", skip_serializing_if = "Option::is_none")]
    pub signature: Option<HashMap<String, String>>,

    /// Creation timestamp in ISO-8601 'basic format' date in UTC
    pub created: String,

    /// Last modified timestamp in ISO-8601 'basic format' date in UTC
    #[serde(rename = "last-modified")]
    pub last_modified: String,

    /// Number of unique package stems in the catalog
    #[serde(rename = "package-count")]
    pub package_count: usize,

    /// Number of unique package versions in the catalog
    #[serde(rename = "package-version-count")]
    pub package_version_count: usize,

    /// Available catalog parts
    pub parts: HashMap<String, CatalogPartInfo>,

    /// Available update logs
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub updates: HashMap<String, UpdateLogInfo>,

    /// Catalog version
    pub version: u32,
}

impl CatalogAttrs {
    /// Create a new catalog attributes structure
    pub fn new() -> Self {
        let now = SystemTime::now();
        let timestamp = format_iso8601_basic(&now);

        CatalogAttrs {
            signature: None,
            created: timestamp.clone(),
            last_modified: timestamp,
            package_count: 0,
            package_version_count: 0,
            parts: HashMap::new(),
            updates: HashMap::new(),
            version: CatalogVersion::V1 as u32,
        }
    }

    /// Save catalog attributes to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load catalog attributes from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let attrs: CatalogAttrs = serde_json::from_str(&json)?;
        Ok(attrs)
    }
}

/// Package version entry in a catalog
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVersionEntry {
    /// Package version string
    pub version: String,

    /// Optional actions associated with this package version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<String>>,

    /// Optional SHA-1 signature of the package manifest
    #[serde(rename = "signature-sha-1", skip_serializing_if = "Option::is_none")]
    pub signature_sha1: Option<String>,
}

/// Catalog part (base, dependency, summary)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPart {
    /// Optional signature information
    #[serde(rename = "_SIGNATURE", skip_serializing_if = "Option::is_none")]
    pub signature: Option<HashMap<String, String>>,

    /// Packages by publisher and stem
    pub packages: HashMap<String, HashMap<String, Vec<PackageVersionEntry>>>,
}

impl CatalogPart {
    /// Create a new catalog part
    pub fn new() -> Self {
        CatalogPart {
            signature: None,
            packages: HashMap::new(),
        }
    }

    /// Add a package to the catalog part
    pub fn add_package(
        &mut self,
        publisher: &str,
        fmri: &Fmri,
        actions: Option<Vec<String>>,
        signature: Option<String>,
    ) {
        let publisher_packages = self
            .packages
            .entry(publisher.to_string())
            .or_insert_with(HashMap::new);
        let stem_versions = publisher_packages
            .entry(fmri.stem().to_string())
            .or_insert_with(Vec::new);

        // Check if this version already exists
        for entry in stem_versions.iter_mut() {
            if !fmri.version().is_empty() && entry.version == fmri.version() {
                // Update existing entry
                if let Some(acts) = actions {
                    entry.actions = Some(acts);
                }
                if let Some(sig) = signature {
                    entry.signature_sha1 = Some(sig);
                }
                return;
            }
        }

        // Add a new entry
        stem_versions.push(PackageVersionEntry {
            version: fmri.version(),
            actions,
            signature_sha1: signature,
        });

        // Sort versions (should be in ascending order)
        stem_versions.sort_by(|a, b| a.version.cmp(&b.version));
    }

    /// Save a catalog part to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load catalog part from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let part: CatalogPart = serde_json::from_str(&json)?;
        Ok(part)
    }
}

/// Operation type for catalog updates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogOperationType {
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "remove")]
    Remove,
}

/// Package update entry in an update log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageUpdateEntry {
    /// Type of operation (add or remove)
    #[serde(rename = "op-type")]
    pub op_type: CatalogOperationType,

    /// Timestamp of the operation in ISO-8601 'basic format' date in UTC
    #[serde(rename = "op-time")]
    pub op_time: String,

    /// Package version string
    pub version: String,

    /// Catalog part entries
    #[serde(flatten)]
    pub catalog_parts: HashMap<String, HashMap<String, Vec<String>>>,

    /// Optional SHA-1 signature of the package manifest
    #[serde(rename = "signature-sha-1", skip_serializing_if = "Option::is_none")]
    pub signature_sha1: Option<String>,
}

/// Update log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateLog {
    /// Optional signature information
    #[serde(rename = "_SIGNATURE", skip_serializing_if = "Option::is_none")]
    pub signature: Option<HashMap<String, String>>,

    /// Updates by publisher and stem
    pub updates: HashMap<String, HashMap<String, Vec<PackageUpdateEntry>>>,
}

impl UpdateLog {
    /// Create a new update log
    pub fn new() -> Self {
        UpdateLog {
            signature: None,
            updates: HashMap::new(),
        }
    }

    /// Add a package update to the log
    pub fn add_update(
        &mut self,
        publisher: &str,
        fmri: &Fmri,
        op_type: CatalogOperationType,
        catalog_parts: HashMap<String, HashMap<String, Vec<String>>>,
        signature: Option<String>,
    ) {
        let publisher_updates = self
            .updates
            .entry(publisher.to_string())
            .or_insert_with(HashMap::new);
        let stem_updates = publisher_updates
            .entry(fmri.stem().to_string())
            .or_insert_with(Vec::new);

        let now = SystemTime::now();
        let timestamp = format_iso8601_basic(&now);

        stem_updates.push(PackageUpdateEntry {
            op_type,
            op_time: timestamp,
            version: fmri.version(),
            catalog_parts,
            signature_sha1: signature,
        });
    }

    /// Save update log to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load update log from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let log: UpdateLog = serde_json::from_str(&json)?;
        Ok(log)
    }
}

/// Catalog manager
pub struct CatalogManager {
    /// Path to the catalog directory
    catalog_dir: PathBuf,

    /// Catalog attributes
    attrs: CatalogAttrs,

    /// Catalog parts
    parts: HashMap<String, CatalogPart>,

    /// Update logs
    update_logs: HashMap<String, UpdateLog>,
}

impl CatalogManager {
    /// Create a new catalog manager
    pub fn new<P: AsRef<Path>>(catalog_dir: P) -> Result<Self> {
        let catalog_dir = catalog_dir.as_ref().to_path_buf();

        // Create catalog directory if it doesn't exist
        if !catalog_dir.exists() {
            fs::create_dir_all(&catalog_dir)?;
        }

        // Try to load existing catalog attributes
        let attrs_path = catalog_dir.join("catalog.attrs");
        let attrs = if attrs_path.exists() {
            CatalogAttrs::load(&attrs_path)?
        } else {
            CatalogAttrs::new()
        };

        Ok(CatalogManager {
            catalog_dir,
            attrs,
            parts: HashMap::new(),
            update_logs: HashMap::new(),
        })
    }

    /// Get catalog attributes
    pub fn attrs(&self) -> &CatalogAttrs {
        &self.attrs
    }

    /// Get mutable catalog attributes
    pub fn attrs_mut(&mut self) -> &mut CatalogAttrs {
        &mut self.attrs
    }

    /// Get a catalog part
    pub fn get_part(&self, name: &str) -> Option<&CatalogPart> {
        self.parts.get(name)
    }

    /// Get a mutable catalog part
    pub fn get_part_mut(&mut self, name: &str) -> Option<&mut CatalogPart> {
        self.parts.get_mut(name)
    }

    /// Load a catalog part
    pub fn load_part(&mut self, name: &str) -> Result<()> {
        let part_path = self.catalog_dir.join(name);
        if part_path.exists() {
            let part = CatalogPart::load(&part_path)?;
            self.parts.insert(name.to_string(), part);
            Ok(())
        } else {
            Err(CatalogError::CatalogPartNotFound {
                name: name.to_string(),
            })
        }
    }

    /// Save a catalog part
    pub fn save_part(&self, name: &str) -> Result<()> {
        if let Some(part) = self.parts.get(name) {
            let part_path = self.catalog_dir.join(name);
            part.save(&part_path)?;
            Ok(())
        } else {
            Err(CatalogError::CatalogPartNotLoaded {
                name: name.to_string(),
            })
        }
    }

    /// Create a new catalog part
    pub fn create_part(&mut self, name: &str) -> &mut CatalogPart {
        self.parts
            .entry(name.to_string())
            .or_insert_with(CatalogPart::new)
    }

    /// Save catalog attributes
    pub fn save_attrs(&self) -> Result<()> {
        let attrs_path = self.catalog_dir.join("catalog.attrs");
        self.attrs.save(&attrs_path)?;
        Ok(())
    }

    /// Create a new update log
    pub fn create_update_log(&mut self, name: &str) -> &mut UpdateLog {
        self.update_logs
            .entry(name.to_string())
            .or_insert_with(UpdateLog::new)
    }

    /// Save an update log
    pub fn save_update_log(&self, name: &str) -> Result<()> {
        if let Some(log) = self.update_logs.get(name) {
            let log_path = self.catalog_dir.join(name);
            log.save(&log_path)?;

            // Update catalog attributes
            let now = SystemTime::now();
            let timestamp = format_iso8601_basic(&now);

            let mut attrs = self.attrs.clone();
            attrs.updates.insert(
                name.to_string(),
                UpdateLogInfo {
                    last_modified: timestamp,
                    signature_sha1: None,
                },
            );

            let attrs_path = self.catalog_dir.join("catalog.attrs");
            attrs.save(&attrs_path)?;

            Ok(())
        } else {
            Err(CatalogError::UpdateLogNotLoaded {
                name: name.to_string(),
            })
        }
    }

    /// Load an update log
    pub fn load_update_log(&mut self, name: &str) -> Result<()> {
        let log_path = self.catalog_dir.join(name);
        if log_path.exists() {
            let log = UpdateLog::load(&log_path)?;
            self.update_logs.insert(name.to_string(), log);
            Ok(())
        } else {
            Err(CatalogError::UpdateLogNotFound {
                name: name.to_string(),
            })
        }
    }

    /// Get an update log
    pub fn get_update_log(&self, name: &str) -> Option<&UpdateLog> {
        self.update_logs.get(name)
    }

    /// Get a mutable update log
    pub fn get_update_log_mut(&mut self, name: &str) -> Option<&mut UpdateLog> {
        self.update_logs.get_mut(name)
    }
}
