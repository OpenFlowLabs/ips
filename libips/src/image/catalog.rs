use crate::actions::Manifest;
use crate::fmri::Fmri;
use crate::repository::catalog::{CatalogManager, CatalogPart, PackageVersionEntry};
use lz4::{Decoder as Lz4Decoder, EncoderBuilder as Lz4EncoderBuilder};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, trace, warn};

/// Errors that can occur when working with the image catalog
#[derive(Error, Debug, Diagnostic)]
pub enum CatalogError {
    #[error("IO error: {0}")]
    #[diagnostic(code(ips::catalog_error::io))]
    IO(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    #[diagnostic(code(ips::catalog_error::json))]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    #[diagnostic(code(ips::catalog_error::database))]
    Database(String),

    #[error("Repository error: {0}")]
    #[diagnostic(code(ips::catalog_error::repository))]
    Repository(#[from] crate::repository::RepositoryError),

    #[error("Action error: {0}")]
    #[diagnostic(code(ips::catalog_error::action))]
    Action(#[from] crate::actions::ActionError),

    #[error("Publisher not found: {0}")]
    #[diagnostic(code(ips::catalog_error::publisher_not_found))]
    PublisherNotFound(String),

    #[error("No publishers configured")]
    #[diagnostic(code(ips::catalog_error::no_publishers))]
    NoPublishers,
}

/// Result type for catalog operations
pub type Result<T> = std::result::Result<T, CatalogError>;

// Internal helpers for (de)compressing manifest JSON payloads stored in redb
fn is_likely_json(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\n' | b'\r' | b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return false;
    }
    matches!(bytes[i], b'{' | b'[')
}

pub(crate) fn compress_json_lz4(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut dst = Vec::with_capacity(bytes.len() / 2 + 32);
    let mut enc = Lz4EncoderBuilder::new()
        .level(4)
        .build(Cursor::new(&mut dst))
        .map_err(|e| CatalogError::Database(format!("Failed to create LZ4 encoder: {}", e)))?;
    enc.write_all(bytes)
        .map_err(|e| CatalogError::Database(format!("Failed to write to LZ4 encoder: {}", e)))?;
    let (_out, res) = enc.finish();
    res.map_err(|e| CatalogError::Database(format!("Failed to finish LZ4 encoding: {}", e)))?;
    Ok(dst)
}

pub(crate) fn decode_manifest_bytes(bytes: &[u8]) -> Result<Manifest> {
    // Fast path: uncompressed legacy JSON
    if is_likely_json(bytes) {
        return Ok(serde_json::from_slice::<Manifest>(bytes)?);
    }
    // Try LZ4 frame decode
    let mut decoder = match Lz4Decoder::new(Cursor::new(bytes)) {
        Ok(d) => d,
        Err(_) => {
            // Fallback: attempt JSON anyway
            return Ok(serde_json::from_slice::<Manifest>(bytes)?);
        }
    };
    let mut out = Vec::new();
    if let Err(_e) = decoder.read_to_end(&mut out) {
        // On decode failure, try JSON as last resort
        return Ok(serde_json::from_slice::<Manifest>(bytes)?);
    }
    Ok(serde_json::from_slice::<Manifest>(&out)?)
}

/// Check if a package manifest is marked as obsolete.
pub(crate) fn is_package_obsolete(manifest: &Manifest) -> bool {
    manifest.attributes.iter().any(|attr| {
        attr.key == "pkg.obsolete" && attr.values.first().map_or(false, |v| v == "true")
    })
}

/// Information about a package in the catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// The FMRI of the package
    pub fmri: Fmri,

    /// Whether the package is obsolete
    pub obsolete: bool,

    /// The publisher of the package
    pub publisher: String,
}

/// The image catalog, which merges catalogs from all publishers
pub struct ImageCatalog {
    /// Path to the catalog database (non-obsolete manifests)
    db_path: PathBuf,
    /// Path to the separate obsoleted database
    obsoleted_db_path: PathBuf,

    /// Path to the catalog directory
    catalog_dir: PathBuf,
}

impl ImageCatalog {
    /// Create a new image catalog
    pub fn new<P: AsRef<Path>>(catalog_dir: P, db_path: P, obsoleted_db_path: P) -> Self {
        ImageCatalog {
            db_path: db_path.as_ref().to_path_buf(),
            obsoleted_db_path: obsoleted_db_path.as_ref().to_path_buf(),
            catalog_dir: catalog_dir.as_ref().to_path_buf(),
        }
    }

    /// Build the catalog from downloaded catalogs
    ///
    /// This method is deprecated in favor of server-side shard building.
    /// For client-side catalog updates, use shard_sync instead.
    pub fn build_catalog(&self, publishers: &[String]) -> Result<()> {
        use tracing::{info, warn};

        info!("Building catalog shards (publishers: {})", publishers.len());

        if publishers.is_empty() {
            return Err(CatalogError::NoPublishers);
        }

        // Get the output directory for shards (parent of db_path)
        let shard_dir = self.db_path.parent().ok_or_else(|| {
            CatalogError::Database("Invalid database path - no parent directory".to_string())
        })?;

        // Process each publisher
        for publisher in publishers {
            let publisher_catalog_dir = self.catalog_dir.join(publisher);

            if !publisher_catalog_dir.exists() {
                warn!(
                    "Publisher catalog directory not found: {}",
                    publisher_catalog_dir.display()
                );
                continue;
            }

            // Determine where catalog parts live
            let nested_dir = publisher_catalog_dir
                .join("publisher")
                .join(publisher)
                .join("catalog");
            let flat_dir = publisher_catalog_dir.clone();

            let catalog_parts_dir = if nested_dir.exists() {
                nested_dir
            } else {
                flat_dir
            };

            if !catalog_parts_dir.exists() {
                warn!(
                    "Catalog parts directory not found: {}",
                    catalog_parts_dir.display()
                );
                continue;
            }

            // Build shards using the new sqlite_catalog module
            crate::repository::sqlite_catalog::build_shards(
                &catalog_parts_dir,
                publisher,
                shard_dir,
            )
            .map_err(|e| {
                CatalogError::Database(format!("Failed to build catalog shards: {}", e.message))
            })?;
        }

        info!("Catalog shards built successfully");
        Ok(())
    }

    // Removed: process_catalog_part - no longer needed, use build_shards() instead
    //
    // Removed: process_publisher_merged - no longer needed, use build_shards() instead
    //
    // Removed: create_or_update_manifest - no longer needed, use build_shards() instead
    //
    // Removed: ensure_fmri_attribute - no longer needed, use build_shards() instead

    /// Check if a package is obsolete (deprecated - use free function is_package_obsolete instead)
    fn is_package_obsolete(&self, manifest: &Manifest) -> bool {
        is_package_obsolete(manifest)
    }

    /// Query the catalog for packages matching a pattern
    ///
    /// Reads from active.db and obsolete.db SQLite shards.
    pub fn query_packages(&self, pattern: Option<&str>) -> Result<Vec<PackageInfo>> {
        use rusqlite::{Connection, OpenFlags};

        let mut results = Vec::new();

        // Query active.db for non-obsolete packages
        if self.db_path.exists() {
            let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| CatalogError::Database(format!("Failed to open active.db: {}", e)))?;

            let mut stmt = conn
                .prepare("SELECT stem, version, publisher FROM packages")
                .map_err(|e| CatalogError::Database(format!("Failed to prepare query: {}", e)))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| CatalogError::Database(format!("Failed to query packages: {}", e)))?;

            for row_result in rows {
                let (stem, version, publisher) = row_result
                    .map_err(|e| CatalogError::Database(format!("Failed to read row: {}", e)))?;

                // Apply pattern filter
                if let Some(pat) = pattern {
                    if !stem.contains(pat) && !version.contains(pat) {
                        continue;
                    }
                }

                // Parse version
                let version_obj = crate::fmri::Version::parse(&version).ok();
                let fmri = Fmri::with_publisher(&publisher, &stem, version_obj);

                results.push(PackageInfo {
                    fmri,
                    obsolete: false,
                    publisher,
                });
            }
        }

        // Query obsolete.db for obsolete packages
        if self.obsoleted_db_path.exists() {
            let conn = Connection::open_with_flags(
                &self.obsoleted_db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| CatalogError::Database(format!("Failed to open obsolete.db: {}", e)))?;

            let mut stmt = conn
                .prepare("SELECT publisher, stem, version FROM obsolete_packages")
                .map_err(|e| CatalogError::Database(format!("Failed to prepare query: {}", e)))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| {
                    CatalogError::Database(format!("Failed to query obsolete packages: {}", e))
                })?;

            for row_result in rows {
                let (publisher, stem, version) = row_result
                    .map_err(|e| CatalogError::Database(format!("Failed to read row: {}", e)))?;

                // Apply pattern filter
                if let Some(pat) = pattern {
                    if !stem.contains(pat) && !version.contains(pat) {
                        continue;
                    }
                }

                // Parse version
                let version_obj = crate::fmri::Version::parse(&version).ok();
                let fmri = Fmri::with_publisher(&publisher, &stem, version_obj);

                results.push(PackageInfo {
                    fmri,
                    obsolete: true,
                    publisher,
                });
            }
        }

        Ok(results)
    }

    /// Get a manifest from the catalog
    ///
    /// Note: The SQLite shards don't store manifests. This method checks if the package
    /// exists in active.db or obsolete.db and returns a minimal manifest if found.
    /// For full manifests, use get_manifest_from_repository or the local manifest cache.
    pub fn get_manifest(&self, fmri: &Fmri) -> Result<Option<Manifest>> {
        use rusqlite::{Connection, OpenFlags};

        // Check active.db
        if self.db_path.exists() {
            let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| CatalogError::Database(format!("Failed to open active.db: {}", e)))?;

            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM packages WHERE stem = ?1 AND version = ?2",
                    rusqlite::params![fmri.stem(), fmri.version()],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if count > 0 {
                // Package exists but we don't have the full manifest in the shard
                // Return a minimal manifest with just the FMRI
                let mut manifest = Manifest::new();
                let mut attr = crate::actions::Attr::default();
                attr.key = "pkg.fmri".to_string();
                attr.values = vec![fmri.to_string()];
                manifest.attributes.push(attr);
                return Ok(Some(manifest));
            }
        }

        // Check obsolete.db
        if self.obsoleted_db_path.exists() {
            let conn = Connection::open_with_flags(
                &self.obsoleted_db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| CatalogError::Database(format!("Failed to open obsolete.db: {}", e)))?;

            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM obsolete_packages WHERE stem = ?1 AND version = ?2",
                    rusqlite::params![fmri.stem(), fmri.version()],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if count > 0 {
                // Create a minimal obsolete manifest
                let mut manifest = Manifest::new();
                let mut attr = crate::actions::Attr::default();
                attr.key = "pkg.fmri".to_string();
                attr.values = vec![fmri.to_string()];
                manifest.attributes.push(attr);

                let mut attr = crate::actions::Attr::default();
                attr.key = "pkg.obsolete".to_string();
                attr.values = vec!["true".to_string()];
                manifest.attributes.push(attr);

                return Ok(Some(manifest));
            }
        }

        Ok(None)
    }

    /// Dump the contents of a specific table to stdout for debugging
    ///
    /// Deprecated: This method used redb. Needs reimplementation for SQLite.
    pub fn dump_table(&self, table_name: &str) -> Result<()> {
        Err(CatalogError::Database(format!(
            "dump_table is not yet implemented for SQLite catalog (requested table: {})",
            table_name
        )))
    }

    /// Dump the contents of all tables to stdout for debugging
    ///
    /// Deprecated: This method used redb. Needs reimplementation for SQLite.
    pub fn dump_all_tables(&self) -> Result<()> {
        Err(CatalogError::Database(
            "dump_all_tables is not yet implemented for SQLite catalog".to_string(),
        ))
    }

    /// Dump the contents of the catalog table (private helper)
    fn dump_catalog_table(&self, _tx: &()) -> Result<()> {
        Err(CatalogError::Database(
            "dump_catalog_table is not yet implemented for SQLite catalog".to_string(),
        ))
    }

    /// Dump the contents of the obsoleted table (private helper)
    fn dump_obsoleted_table(&self, _tx: &()) -> Result<()> {
        Err(CatalogError::Database(
            "dump_obsoleted_table is not yet implemented for SQLite catalog".to_string(),
        ))
    }

    /// Get database statistics
    ///
    /// Deprecated: This method used redb. Needs reimplementation for SQLite.
    pub fn get_db_stats(&self) -> Result<()> {
        Err(CatalogError::Database(
            "get_db_stats is not yet implemented for SQLite catalog".to_string(),
        ))
    }
}

// Removed all the implementation code for the following methods since they're no longer used:
// - process_catalog_part
// - process_publisher_merged
// - create_or_update_manifest
// - ensure_fmri_attribute

/// Helper function to parse an action string and extract the key-value pairs
#[allow(dead_code)]
fn parse_action(_action_str: &str) -> HashMap<String, String> {
    // This is legacy code for the old catalog building. It's not needed anymore.
    HashMap::new()
}
