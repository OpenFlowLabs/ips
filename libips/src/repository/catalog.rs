//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
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
    CatalogPartNotFound { name: String },

    #[error("catalog part not loaded: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::part_not_loaded),
        help("Load the catalog part before attempting to save it")
    )]
    CatalogPartNotLoaded { name: String },

    #[error("update log not loaded: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::update_log_not_loaded),
        help("Load the update log before attempting to save it")
    )]
    UpdateLogNotLoaded { name: String },

    #[error("update log does not exist: {name}")]
    #[diagnostic(
        code(ips::repository_error::catalog::update_log_not_found),
        help("Check that the update log exists and is accessible")
    )]
    UpdateLogNotFound { name: String },

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
pub fn format_iso8601_basic(time: &SystemTime) -> String {
    let datetime = convert_system_time_to_datetime(time);
    let micros = datetime.timestamp_subsec_micros();
    format!("{}.{:06}Z", datetime.format("%Y%m%dT%H%M%S"), micros)
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
    pub parts: BTreeMap<String, CatalogPartInfo>,

    /// Available update logs
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub updates: BTreeMap<String, UpdateLogInfo>,

    /// Catalog version
    pub version: u32,

    /// Optional signature information
    #[serde(rename = "_SIGNATURE", skip_serializing_if = "Option::is_none")]
    pub signature: Option<BTreeMap<String, String>>,
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
            parts: BTreeMap::new(),
            updates: BTreeMap::new(),
            version: CatalogVersion::V1 as u32,
        }
    }

    /// Save catalog attributes to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string(self)?;
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
    /// Optional actions associated with this package version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<String>>,

    /// Optional SHA-1 signature of the package manifest
    #[serde(rename = "signature-sha-1", skip_serializing_if = "Option::is_none")]
    pub signature_sha1: Option<String>,

    /// Package version string
    pub version: String,
}

/// Catalog part (base, dependency, summary)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogPart {
    /// Packages by publisher and stem
    #[serde(flatten)]
    pub packages: BTreeMap<String, BTreeMap<String, Vec<PackageVersionEntry>>>,

    /// Metadata fields (keys starting with '_')
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for CatalogPart {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let all_entries = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let mut packages = BTreeMap::new();
        let mut metadata = BTreeMap::new();

        for (k, v) in all_entries {
            if k.starts_with('_') {
                metadata.insert(k, v);
            } else {
                // Try to parse as package map
                if let Ok(pkg_map) = serde_json::from_value(v.clone()) {
                    packages.insert(k, pkg_map);
                } else {
                    // If it fails, treat as metadata
                    metadata.insert(k, v);
                }
            }
        }

        Ok(CatalogPart { packages, metadata })
    }
}

impl CatalogPart {
    /// Create a new catalog part
    pub fn new() -> Self {
        CatalogPart {
            packages: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Get signature information if present
    pub fn signature(&self) -> Option<BTreeMap<String, String>> {
        self.metadata.get("_SIGNATURE").and_then(|v| {
            serde_json::from_value::<BTreeMap<String, String>>(v.clone()).ok()
        })
    }

    /// Set signature information
    pub fn set_signature(&mut self, signature: Option<BTreeMap<String, String>>) {
        if let Some(sig) = signature {
            if let Ok(val) = serde_json::to_value(sig) {
                self.metadata.insert("_SIGNATURE".to_string(), val);
            }
        } else {
            self.metadata.remove("_SIGNATURE");
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
            .or_insert_with(BTreeMap::new);
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
        let path_ref = path.as_ref();
        let json = fs::File::open(path_ref)?;

        // Try to parse the JSON directly
        match serde_json::from_reader(json) {
            Ok(part) => Ok(part),
            Err(e) => Err(CatalogError::JsonSerializationError(e)),
        }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    pub catalog_parts: BTreeMap<String, BTreeMap<String, Vec<String>>>,

    /// Optional SHA-1 signature of the package manifest
    #[serde(rename = "signature-sha-1", skip_serializing_if = "Option::is_none")]
    pub signature_sha1: Option<String>,

    /// Metadata fields (keys starting with '_')
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for PackageUpdateEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Internal {
            #[serde(rename = "op-type")]
            op_type: CatalogOperationType,
            #[serde(rename = "op-time")]
            op_time: String,
            version: String,
            #[serde(rename = "signature-sha-1")]
            signature_sha1: Option<String>,
            #[serde(flatten)]
            all_entries: BTreeMap<String, Value>,
        }

        let internal = Internal::deserialize(deserializer)?;
        let mut catalog_parts = BTreeMap::new();
        let mut metadata = BTreeMap::new();

        for (k, v) in internal.all_entries {
            if k.starts_with('_') {
                metadata.insert(k, v);
            } else {
                if let Ok(part_map) = serde_json::from_value(v.clone()) {
                    catalog_parts.insert(k, part_map);
                } else {
                    metadata.insert(k, v);
                }
            }
        }

        Ok(PackageUpdateEntry {
            op_type: internal.op_type,
            op_time: internal.op_time,
            version: internal.version,
            catalog_parts,
            signature_sha1: internal.signature_sha1,
            metadata,
        })
    }
}

/// Update log
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateLog {
    /// Updates by publisher and stem
    pub updates: BTreeMap<String, BTreeMap<String, Vec<PackageUpdateEntry>>>,

    /// Metadata fields (keys starting with '_')
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for UpdateLog {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Internal {
            #[serde(default)]
            updates: BTreeMap<String, BTreeMap<String, Vec<PackageUpdateEntry>>>,
            #[serde(flatten)]
            all_entries: BTreeMap<String, Value>,
        }

        let internal = Internal::deserialize(deserializer)?;
        let mut metadata = internal.all_entries;
        metadata.remove("updates");

        Ok(UpdateLog {
            updates: internal.updates,
            metadata,
        })
    }
}

impl UpdateLog {
    /// Create a new update log
    pub fn new() -> Self {
        UpdateLog {
            updates: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Get signature information if present
    pub fn signature(&self) -> Option<BTreeMap<String, String>> {
        self.metadata.get("_SIGNATURE").and_then(|v| {
            serde_json::from_value::<BTreeMap<String, String>>(v.clone()).ok()
        })
    }

    /// Set signature information
    pub fn set_signature(&mut self, signature: Option<BTreeMap<String, String>>) {
        if let Some(sig) = signature {
            if let Ok(val) = serde_json::to_value(sig) {
                self.metadata.insert("_SIGNATURE".to_string(), val);
            }
        } else {
            self.metadata.remove("_SIGNATURE");
        }
    }

    /// Add a package update to the log
    pub fn add_update(
        &mut self,
        publisher: &str,
        fmri: &Fmri,
        op_type: CatalogOperationType,
        catalog_parts: BTreeMap<String, BTreeMap<String, Vec<String>>>,
        signature: Option<String>,
    ) {
        let publisher_updates = self
            .updates
            .entry(publisher.to_string())
            .or_insert_with(BTreeMap::new);
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
            metadata: BTreeMap::new(),
        });
    }

    /// Save update log to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string(self)?;
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

    /// Publisher name
    publisher: String,

    /// Catalog attributes
    attrs: CatalogAttrs,

    /// Catalog parts
    parts: BTreeMap<String, CatalogPart>,

    /// Update logs
    update_logs: BTreeMap<String, UpdateLog>,
}

impl CatalogManager {
    /// Create a new catalog manager
    pub fn new<P: AsRef<Path>>(base_dir: P, publisher: &str) -> Result<Self> {
        let publisher_catalog_dir = base_dir.as_ref().to_path_buf();

        // Create catalog directory if it doesn't exist
        if !publisher_catalog_dir.exists() {
            fs::create_dir_all(&publisher_catalog_dir)?;
        }

        // Try to load existing catalog attributes
        let attrs_path = publisher_catalog_dir.join("catalog.attrs");
        let attrs = if attrs_path.exists() {
            CatalogAttrs::load(&attrs_path)?
        } else {
            CatalogAttrs::new()
        };

        Ok(CatalogManager {
            catalog_dir: publisher_catalog_dir,
            publisher: publisher.to_string(),
            attrs,
            parts: BTreeMap::new(),
            update_logs: BTreeMap::new(),
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

    /// Add a package to a catalog part using the stored publisher
    pub fn add_package_to_part(
        &mut self,
        part_name: &str,
        fmri: &Fmri,
        actions: Option<Vec<String>>,
        signature: Option<String>,
    ) -> Result<()> {
        if let Some(part) = self.parts.get_mut(part_name) {
            part.add_package(&self.publisher, fmri, actions, signature);
            Ok(())
        } else {
            Err(CatalogError::CatalogPartNotLoaded {
                name: part_name.to_string(),
            })
        }
    }

    /// Add an update to an update log using the stored publisher
    pub fn add_update_to_log(
        &mut self,
        log_name: &str,
        fmri: &Fmri,
        op_type: CatalogOperationType,
        catalog_parts: BTreeMap<String, BTreeMap<String, Vec<String>>>,
        signature: Option<String>,
    ) -> Result<()> {
        if let Some(log) = self.update_logs.get_mut(log_name) {
            log.add_update(&self.publisher, fmri, op_type, catalog_parts, signature);
            Ok(())
        } else {
            Err(CatalogError::UpdateLogNotLoaded {
                name: log_name.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_load_sample_catalog() {
        // Path is relative to the crate root (libips)
        let path = PathBuf::from(
            "../sample_data/sample-repo/publisher/openindiana.org/catalog/catalog.base.C",
        );

        // Only run this test if the sample data exists
        if path.exists() {
            println!("Testing with sample catalog at {:?}", path);
            match CatalogPart::load(&path) {
                Ok(part) => {
                    println!("Successfully loaded catalog part");

                    // Verify we loaded the correct data structure
                    // The sample file has "openindiana.org" as a key
                    assert!(
                        part.packages.contains_key("openindiana.org"),
                        "Catalog should contain openindiana.org publisher"
                    );

                    let packages = part.packages.get("openindiana.org").unwrap();
                    println!("Found {} packages for openindiana.org", packages.len());
                    assert!(!packages.is_empty(), "Should have loaded packages");
                }
                Err(e) => panic!("Failed to load catalog part: {}", e),
            }
        } else {
            println!(
                "Sample data not found at {:?}, skipping test. This is expected in some CI environments.",
                path
            );
        }
    }
}
