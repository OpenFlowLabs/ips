use crate::actions::Manifest;
use crate::fmri::Fmri;
use crate::repository::catalog::{CatalogManager, CatalogPart, PackageVersionEntry};
use lz4::{Decoder as Lz4Decoder, EncoderBuilder as Lz4EncoderBuilder};
use miette::Diagnostic;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, trace, warn};

/// Table definition for the catalog database
/// Key: stem@version
/// Value: serialized Manifest
pub const CATALOG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("catalog");

/// Table definition for the obsoleted packages catalog
/// Key: full FMRI including publisher (pkg://publisher/stem@version)
/// Value: nothing
pub const OBSOLETED_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("obsoleted");

/// Table definition for the incorporate locks table
/// Key: stem (e.g., "compress/gzip")
/// Value: version string as bytes (same format as Fmri::version())
pub const INCORPORATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("incorporate");

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

fn compress_json_lz4(bytes: &[u8]) -> Result<Vec<u8>> {
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

fn decode_manifest_bytes(bytes: &[u8]) -> Result<Manifest> {
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

    /// Dump the contents of a specific table to stdout for debugging
    pub fn dump_table(&self, table_name: &str) -> Result<()> {
        // Determine which table to dump and open the appropriate database
        match table_name {
            "catalog" => {
                let db = Database::open(&self.db_path).map_err(|e| {
                    CatalogError::Database(format!("Failed to open catalog database: {}", e))
                })?;
                let tx = db.begin_read().map_err(|e| {
                    CatalogError::Database(format!("Failed to begin transaction: {}", e))
                })?;
                self.dump_catalog_table(&tx)?;
            }
            "obsoleted" => {
                let db = Database::open(&self.obsoleted_db_path).map_err(|e| {
                    CatalogError::Database(format!("Failed to open obsoleted database: {}", e))
                })?;
                let tx = db.begin_read().map_err(|e| {
                    CatalogError::Database(format!("Failed to begin transaction: {}", e))
                })?;
                self.dump_obsoleted_table(&tx)?;
            }
            "incorporate" => {
                let db = Database::open(&self.db_path).map_err(|e| {
                    CatalogError::Database(format!("Failed to open catalog database: {}", e))
                })?;
                let tx = db.begin_read().map_err(|e| {
                    CatalogError::Database(format!("Failed to begin transaction: {}", e))
                })?;
                // Simple dump of incorporate locks
                if let Ok(table) = tx.open_table(INCORPORATE_TABLE) {
                    for entry in table.iter().map_err(|e| {
                        CatalogError::Database(format!(
                            "Failed to iterate incorporate table: {}",
                            e
                        ))
                    })? {
                        let (k, v) = entry.map_err(|e| {
                            CatalogError::Database(format!(
                                "Failed to read incorporate table entry: {}",
                                e
                            ))
                        })?;
                        let stem = k.value();
                        let ver = String::from_utf8_lossy(v.value());
                        println!("{} -> {}", stem, ver);
                    }
                }
            }
            _ => {
                return Err(CatalogError::Database(format!(
                    "Unknown table: {}",
                    table_name
                )));
            }
        }

        Ok(())
    }

    /// Dump the contents of all tables to stdout for debugging
    pub fn dump_all_tables(&self) -> Result<()> {
        // Catalog DB
        let db_cat = Database::open(&self.db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open catalog database: {}", e))
        })?;
        let tx_cat = db_cat
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        println!("=== CATALOG TABLE ===");
        let _ = self.dump_catalog_table(&tx_cat);

        // Obsoleted DB
        let db_obs = Database::open(&self.obsoleted_db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted database: {}", e))
        })?;
        let tx_obs = db_obs
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        println!("\n=== OBSOLETED TABLE ===");
        let _ = self.dump_obsoleted_table(&tx_obs);

        Ok(())
    }

    /// Dump the contents of the catalog table
    fn dump_catalog_table(&self, tx: &redb::ReadTransaction) -> Result<()> {
        match tx.open_table(CATALOG_TABLE) {
            Ok(table) => {
                let mut count = 0;
                for entry_result in table.iter().map_err(|e| {
                    CatalogError::Database(format!("Failed to iterate catalog table: {}", e))
                })? {
                    let (key, value) = entry_result.map_err(|e| {
                        CatalogError::Database(format!(
                            "Failed to get entry from catalog table: {}",
                            e
                        ))
                    })?;
                    let key_str = key.value();

                    // Try to deserialize the manifest (supports JSON or LZ4-compressed JSON)
                    match decode_manifest_bytes(value.value()) {
                        Ok(manifest) => {
                            // Extract the publisher from the FMRI attribute
                            let publisher = manifest
                                .attributes
                                .iter()
                                .find(|attr| attr.key == "pkg.fmri")
                                .and_then(|attr| attr.values.get(0).cloned())
                                .unwrap_or_else(|| "unknown".to_string());

                            println!("Key: {}", key_str);
                            println!("  FMRI: {}", publisher);
                            println!("  Attributes: {}", manifest.attributes.len());
                            println!("  Files: {}", manifest.files.len());
                            println!("  Directories: {}", manifest.directories.len());
                            println!("  Dependencies: {}", manifest.dependencies.len());
                        }
                        Err(e) => {
                            println!("Key: {}", key_str);
                            println!("  Error deserializing manifest: {}", e);
                        }
                    }
                    count += 1;
                }
                println!("Total entries in catalog table: {}", count);
                Ok(())
            }
            Err(e) => {
                println!("Error opening catalog table: {}", e);
                Err(CatalogError::Database(format!(
                    "Failed to open catalog table: {}",
                    e
                )))
            }
        }
    }

    /// Dump the contents of the obsoleted table
    fn dump_obsoleted_table(&self, tx: &redb::ReadTransaction) -> Result<()> {
        match tx.open_table(OBSOLETED_TABLE) {
            Ok(table) => {
                let mut count = 0;
                for entry_result in table.iter().map_err(|e| {
                    CatalogError::Database(format!("Failed to iterate obsoleted table: {}", e))
                })? {
                    let (key, _) = entry_result.map_err(|e| {
                        CatalogError::Database(format!(
                            "Failed to get entry from obsoleted table: {}",
                            e
                        ))
                    })?;
                    let key_str = key.value();

                    println!("Key: {}", key_str);
                    count += 1;
                }
                println!("Total entries in obsoleted table: {}", count);
                Ok(())
            }
            Err(e) => {
                println!("Error opening obsoleted table: {}", e);
                Err(CatalogError::Database(format!(
                    "Failed to open obsoleted table: {}",
                    e
                )))
            }
        }
    }

    /// Get database statistics
    pub fn get_db_stats(&self) -> Result<()> {
        // Open the catalog database
        let db_cat = Database::open(&self.db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open catalog database: {}", e))
        })?;
        let tx_cat = db_cat
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Open the obsoleted database
        let db_obs = Database::open(&self.obsoleted_db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted database: {}", e))
        })?;
        let tx_obs = db_obs
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Get table statistics
        let mut catalog_count = 0;
        let mut obsoleted_count = 0;

        // Count catalog entries
        if let Ok(table) = tx_cat.open_table(CATALOG_TABLE) {
            for result in table.iter().map_err(|e| {
                CatalogError::Database(format!("Failed to iterate catalog table: {}", e))
            })? {
                let _ = result.map_err(|e| {
                    CatalogError::Database(format!("Failed to get entry from catalog table: {}", e))
                })?;
                catalog_count += 1;
            }
        }

        // Count obsoleted entries (separate DB)
        if let Ok(table) = tx_obs.open_table(OBSOLETED_TABLE) {
            for result in table.iter().map_err(|e| {
                CatalogError::Database(format!("Failed to iterate obsoleted table: {}", e))
            })? {
                let _ = result.map_err(|e| {
                    CatalogError::Database(format!(
                        "Failed to get entry from obsoleted table: {}",
                        e
                    ))
                })?;
                obsoleted_count += 1;
            }
        }

        // Print statistics
        println!("Catalog database path: {}", self.db_path.display());
        println!(
            "Obsoleted database path: {}",
            self.obsoleted_db_path.display()
        );
        println!("Catalog directory: {}", self.catalog_dir.display());
        println!("Table statistics:");
        println!("  Catalog table: {} entries", catalog_count);
        println!("  Obsoleted table: {} entries", obsoleted_count);
        println!("Total entries: {}", catalog_count + obsoleted_count);

        Ok(())
    }

    /// Initialize the catalog database
    pub fn init_db(&self) -> Result<()> {
        // Ensure parent directories exist
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.obsoleted_db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create/open catalog database and tables
        let db_cat = Database::create(&self.db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to create catalog database: {}", e))
        })?;
        let tx_cat = db_cat
            .begin_write()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        tx_cat.open_table(CATALOG_TABLE).map_err(|e| {
            CatalogError::Database(format!("Failed to create catalog table: {}", e))
        })?;
        tx_cat.open_table(INCORPORATE_TABLE).map_err(|e| {
            CatalogError::Database(format!("Failed to create incorporate table: {}", e))
        })?;
        tx_cat.commit().map_err(|e| {
            CatalogError::Database(format!("Failed to commit catalog transaction: {}", e))
        })?;

        // Create/open obsoleted database and table
        let db_obs = Database::create(&self.obsoleted_db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to create obsoleted database: {}", e))
        })?;
        let tx_obs = db_obs
            .begin_write()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        tx_obs.open_table(OBSOLETED_TABLE).map_err(|e| {
            CatalogError::Database(format!("Failed to create obsoleted table: {}", e))
        })?;
        tx_obs.commit().map_err(|e| {
            CatalogError::Database(format!("Failed to commit obsoleted transaction: {}", e))
        })?;

        Ok(())
    }

    /// Build the catalog from downloaded catalogs
    pub fn build_catalog(&self, publishers: &[String]) -> Result<()> {
        info!("Building catalog (publishers: {})", publishers.len());
        trace!("Catalog directory: {:?}", self.catalog_dir);
        trace!("Catalog database path: {:?}", self.db_path);

        if publishers.is_empty() {
            return Err(CatalogError::NoPublishers);
        }

        // Open the databases
        trace!(
            "Opening databases at {:?} and {:?}",
            self.db_path, self.obsoleted_db_path
        );
        let db_cat = Database::open(&self.db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open catalog database: {}", e))
        })?;
        let db_obs = Database::open(&self.obsoleted_db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted database: {}", e))
        })?;

        // Begin writing transactions
        trace!("Beginning write transactions");
        let tx_cat = db_cat.begin_write().map_err(|e| {
            CatalogError::Database(format!("Failed to begin catalog transaction: {}", e))
        })?;
        let tx_obs = db_obs.begin_write().map_err(|e| {
            CatalogError::Database(format!("Failed to begin obsoleted transaction: {}", e))
        })?;

        // Open the catalog table
        trace!("Opening catalog table");
        let mut catalog_table = tx_cat
            .open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open catalog table: {}", e)))?;

        // Open the obsoleted table
        trace!("Opening obsoleted table");
        let mut obsoleted_table = tx_obs.open_table(OBSOLETED_TABLE).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted table: {}", e))
        })?;

        // Process each publisher
        for publisher in publishers {
            trace!("Processing publisher: {}", publisher);
            let publisher_catalog_dir = self.catalog_dir.join(publisher);
            trace!("Publisher catalog directory: {:?}", publisher_catalog_dir);

            // Skip if the publisher catalog directory doesn't exist
            if !publisher_catalog_dir.exists() {
                warn!(
                    "Publisher catalog directory not found: {}",
                    publisher_catalog_dir.display()
                );
                continue;
            }

            // Determine where catalog parts live. Support both legacy nested layout
            // (publisher/<publisher>/catalog) and flat layout (directly under publisher dir).
            let nested_dir = publisher_catalog_dir
                .join("publisher")
                .join(publisher)
                .join("catalog");
            let flat_dir = publisher_catalog_dir.clone();

            let catalog_parts_dir = if nested_dir.exists() {
                &nested_dir
            } else {
                &flat_dir
            };

            trace!("Creating catalog manager for publisher: {}", publisher);
            trace!("Catalog parts directory: {:?}", catalog_parts_dir);

            // Check if the catalog parts directory exists (either layout)
            if !catalog_parts_dir.exists() {
                warn!(
                    "Catalog parts directory not found: {}",
                    catalog_parts_dir.display()
                );
                continue;
            }

            let mut catalog_manager =
                CatalogManager::new(catalog_parts_dir, publisher).map_err(|e| {
                    CatalogError::Repository(crate::repository::RepositoryError::Other(format!(
                        "Failed to create catalog manager: {}",
                        e
                    )))
                })?;

            // Get all catalog parts
            trace!("Getting catalog parts for publisher: {}", publisher);
            let parts = catalog_manager.attrs().parts.clone();
            trace!("Catalog parts: {:?}", parts.keys().collect::<Vec<_>>());

            // Load all catalog parts
            for part_name in parts.keys() {
                trace!("Loading catalog part: {}", part_name);
                catalog_manager.load_part(part_name).map_err(|e| {
                    CatalogError::Repository(crate::repository::RepositoryError::Other(format!(
                        "Failed to load catalog part: {}",
                        e
                    )))
                })?;
            }

            // New approach: Merge information across all catalog parts per stem@version, then process once
            let mut loaded_parts: Vec<&CatalogPart> = Vec::new();
            for part_name in parts.keys() {
                if let Some(part) = catalog_manager.get_part(part_name) {
                    loaded_parts.push(part);
                }
            }
            self.process_publisher_merged(
                &mut catalog_table,
                &mut obsoleted_table,
                publisher,
                &loaded_parts,
            )?;
        }

        // Drop the tables to release the borrow on tx
        drop(catalog_table);
        drop(obsoleted_table);

        // Commit the transactions
        tx_cat.commit().map_err(|e| {
            CatalogError::Database(format!("Failed to commit catalog transaction: {}", e))
        })?;
        tx_obs.commit().map_err(|e| {
            CatalogError::Database(format!("Failed to commit obsoleted transaction: {}", e))
        })?;

        info!("Catalog built successfully");
        Ok(())
    }

    /// Process a catalog part and add its packages to the catalog
    #[allow(dead_code)]
    fn process_catalog_part(
        &self,
        catalog_table: &mut redb::Table<&str, &[u8]>,
        obsoleted_table: &mut redb::Table<&str, &[u8]>,
        part_name: &str,
        part: &CatalogPart,
        publisher: &str,
    ) -> Result<()> {
        trace!("Processing catalog part for publisher: {}", publisher);

        // Get packages for this publisher
        if let Some(publisher_packages) = part.packages.get(publisher) {
            let total_versions: usize = publisher_packages.values().map(|v| v.len()).sum();
            let mut processed: usize = 0;
            // Count of packages marked obsolete in this part, including those skipped because they were already marked obsolete in earlier parts.
            let mut obsolete_count_incl_skipped: usize = 0;
            let mut skipped_obsolete: usize = 0;
            let progress_step: usize = 500; // report every N packages

            trace!(
                "Found {} package stems ({} versions) for publisher {}",
                publisher_packages.len(),
                total_versions,
                publisher
            );

            // Process each package stem
            for (stem, versions) in publisher_packages {
                trace!(
                    "Processing package stem: {} ({} versions)",
                    stem,
                    versions.len()
                );

                // Process each package version
                for version_entry in versions {
                    trace!(
                        "Processing version: {} | actions: {:?}",
                        version_entry.version, version_entry.actions
                    );

                    // Create the FMRI
                    let version = if !version_entry.version.is_empty() {
                        match crate::fmri::Version::parse(&version_entry.version) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                warn!("Failed to parse version '{}': {}", version_entry.version, e);
                                continue;
                            }
                        }
                    } else {
                        None
                    };

                    let fmri = Fmri::with_publisher(publisher, stem, version);
                    let catalog_key = format!("{}@{}", stem, version_entry.version);
                    let obsoleted_key = fmri.to_string();

                    // If this is not the base part and this package/version was already marked
                    // obsolete in an earlier part (present in obsoleted_table) and is NOT present
                    // in the catalog_table, skip importing it from this part.
                    if !part_name.contains(".base") {
                        let has_catalog =
                            matches!(catalog_table.get(catalog_key.as_str()), Ok(Some(_)));
                        if !has_catalog {
                            let was_obsoleted =
                                matches!(obsoleted_table.get(obsoleted_key.as_str()), Ok(Some(_)));
                            if was_obsoleted {
                                // Count as obsolete for progress accounting, even though we skip processing
                                obsolete_count_incl_skipped += 1;
                                skipped_obsolete += 1;
                                trace!(
                                    "Skipping {} from part {} because it is marked obsolete and not present in catalog",
                                    obsoleted_key, part_name
                                );
                                continue;
                            }
                        }
                    }

                    // Check if we already have this package in the catalog
                    let existing_manifest = match catalog_table.get(catalog_key.as_str()) {
                        Ok(Some(bytes)) => Some(decode_manifest_bytes(bytes.value())?),
                        _ => None,
                    };

                    // Create or update the manifest
                    let manifest = self.create_or_update_manifest(
                        existing_manifest,
                        version_entry,
                        stem,
                        publisher,
                    )?;

                    // Check if the package is obsolete
                    let is_obsolete = self.is_package_obsolete(&manifest);
                    if is_obsolete {
                        obsolete_count_incl_skipped += 1;
                    }

                    // Serialize the manifest
                    let manifest_bytes = serde_json::to_vec(&manifest)?;

                    // Store the package in the appropriate table
                    if is_obsolete {
                        // Store obsolete packages in the obsoleted table with the full FMRI as key
                        let empty_bytes: &[u8] = &[0u8; 0];
                        obsoleted_table
                            .insert(obsoleted_key.as_str(), empty_bytes)
                            .map_err(|e| {
                                CatalogError::Database(format!(
                                    "Failed to insert into obsoleted table: {}",
                                    e
                                ))
                            })?;
                    } else {
                        // Store non-obsolete packages in the catalog table with stem@version as a key
                        let compressed = compress_json_lz4(&manifest_bytes)?;
                        catalog_table
                            .insert(catalog_key.as_str(), compressed.as_slice())
                            .map_err(|e| {
                                CatalogError::Database(format!(
                                    "Failed to insert into catalog table: {}",
                                    e
                                ))
                            })?;
                    }

                    processed += 1;
                    if processed % progress_step == 0 {
                        info!(
                            "Import progress (publisher {}, part {}): {}/{} versions processed ({} obsolete incl. skipped, {} skipped)",
                            publisher,
                            part_name,
                            processed,
                            total_versions,
                            obsolete_count_incl_skipped,
                            skipped_obsolete
                        );
                    }
                }
            }

            // Final summary for this part/publisher
            info!(
                "Finished import for publisher {}, part {}: {} versions processed ({} obsolete incl. skipped, {} skipped)",
                publisher, part_name, processed, obsolete_count_incl_skipped, skipped_obsolete
            );
        } else {
            trace!("No packages found for publisher: {}", publisher);
        }

        Ok(())
    }

    /// Process all catalog parts by merging entries per stem@version and deciding once per package
    fn process_publisher_merged(
        &self,
        catalog_table: &mut redb::Table<&str, &[u8]>,
        obsoleted_table: &mut redb::Table<&str, &[u8]>,
        publisher: &str,
        parts: &[&CatalogPart],
    ) -> Result<()> {
        trace!("Processing merged catalog for publisher: {}", publisher);

        // Build merged map: stem -> version -> PackageVersionEntry (with merged actions/signature)
        let mut merged: HashMap<String, HashMap<String, PackageVersionEntry>> = HashMap::new();

        for part in parts {
            if let Some(publisher_packages) = part.packages.get(publisher) {
                for (stem, versions) in publisher_packages {
                    let stem_map = merged.entry(stem.clone()).or_default();
                    for v in versions {
                        let entry =
                            stem_map
                                .entry(v.version.clone())
                                .or_insert(PackageVersionEntry {
                                    version: v.version.clone(),
                                    actions: None,
                                    signature_sha1: None,
                                });
                        // Merge signature if not yet set
                        if entry.signature_sha1.is_none() {
                            if let Some(sig) = &v.signature_sha1 {
                                entry.signature_sha1 = Some(sig.clone());
                            }
                        }
                        // Merge actions, de-duplicating
                        if let Some(actions) = &v.actions {
                            let ea = entry.actions.get_or_insert_with(Vec::new);
                            for a in actions {
                                if !ea.contains(a) {
                                    ea.push(a.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Compute totals for progress logging
        let total_versions: usize = merged.values().map(|m| m.len()).sum();
        let mut processed: usize = 0;
        let mut obsolete_count: usize = 0;
        let progress_step: usize = 500;

        // Deterministic order: sort stems and versions
        let mut stems: Vec<&String> = merged.keys().collect();
        stems.sort();

        for stem in stems {
            if let Some(versions_map) = merged.get(stem) {
                let mut versions: Vec<&String> = versions_map.keys().collect();
                versions.sort();

                for ver in versions {
                    let entry = versions_map.get(ver).expect("version entry exists");

                    // Keys
                    let catalog_key = format!("{}@{}", stem, entry.version);

                    // Read existing manifest if present
                    let existing_manifest = match catalog_table.get(catalog_key.as_str()) {
                        Ok(Some(bytes)) => Some(decode_manifest_bytes(bytes.value())?),
                        _ => None,
                    };

                    // Build/update manifest with merged actions
                    let manifest =
                        self.create_or_update_manifest(existing_manifest, entry, stem, publisher)?;

                    // Obsolete decision based on merged actions in manifest
                    let is_obsolete = self.is_package_obsolete(&manifest);
                    if is_obsolete {
                        obsolete_count += 1;
                    }

                    // Serialize and write
                    if is_obsolete {
                        // Compute full FMRI for obsoleted key
                        let version_obj = if !entry.version.is_empty() {
                            match crate::fmri::Version::parse(&entry.version) {
                                Ok(v) => Some(v),
                                Err(_) => None,
                            }
                        } else {
                            None
                        };
                        let fmri = Fmri::with_publisher(publisher, stem, version_obj);
                        let obsoleted_key = fmri.to_string();
                        let empty_bytes: &[u8] = &[0u8; 0];
                        obsoleted_table
                            .insert(obsoleted_key.as_str(), empty_bytes)
                            .map_err(|e| {
                                CatalogError::Database(format!(
                                    "Failed to insert into obsoleted table: {}",
                                    e
                                ))
                            })?;
                    } else {
                        let manifest_bytes = serde_json::to_vec(&manifest)?;
                        let compressed = compress_json_lz4(&manifest_bytes)?;
                        catalog_table
                            .insert(catalog_key.as_str(), compressed.as_slice())
                            .map_err(|e| {
                                CatalogError::Database(format!(
                                    "Failed to insert into catalog table: {}",
                                    e
                                ))
                            })?;
                    }

                    processed += 1;
                    if processed % progress_step == 0 {
                        info!(
                            "Import progress (publisher {}, merged): {}/{} versions processed ({} obsolete)",
                            publisher, processed, total_versions, obsolete_count
                        );
                    }
                }
            }
        }

        info!(
            "Finished merged import for publisher {}: {} versions processed ({} obsolete)",
            publisher, processed, obsolete_count
        );

        Ok(())
    }

    /// Create or update a manifest from a package version entry
    fn create_or_update_manifest(
        &self,
        existing_manifest: Option<Manifest>,
        version_entry: &PackageVersionEntry,
        stem: &str,
        publisher: &str,
    ) -> Result<Manifest> {
        // Start with the existing manifest or create a new one
        let mut manifest = existing_manifest.unwrap_or_else(Manifest::new);

        // Parse and add actions from the version entry
        if let Some(actions) = &version_entry.actions {
            for action_str in actions {
                // Parse each action string to extract attributes we care about in the catalog
                if action_str.starts_with("set ") {
                    // Format is typically "set name=pkg.key value=value"
                    if let Some(name_part) = action_str.split_whitespace().nth(1) {
                        if name_part.starts_with("name=") {
                            // Extract the key (after "name=")
                            let key = &name_part[5..];

                            // Extract the value (after "value=")
                            if let Some(value_part) = action_str.split_whitespace().nth(2) {
                                if value_part.starts_with("value=") {
                                    let mut value = &value_part[6..];

                                    // Remove quotes if present
                                    if value.starts_with('"') && value.ends_with('"') {
                                        value = &value[1..value.len() - 1];
                                    }

                                    // Add or update the attribute in the manifest
                                    let attr_index =
                                        manifest.attributes.iter().position(|attr| attr.key == key);
                                    if let Some(index) = attr_index {
                                        manifest.attributes[index].values = vec![value.to_string()];
                                    } else {
                                        let mut attr = crate::actions::Attr::default();
                                        attr.key = key.to_string();
                                        attr.values = vec![value.to_string()];
                                        manifest.attributes.push(attr);
                                    }
                                }
                            }
                        }
                    }
                } else if action_str.starts_with("depend ") {
                    // Example: "depend fmri=desktop/mate/caja type=require"
                    let rest = &action_str[7..]; // strip leading "depend "
                    let mut dep_type: String = String::new();
                    let mut dep_predicate: Option<crate::fmri::Fmri> = None;
                    let mut dep_fmris: Vec<crate::fmri::Fmri> = Vec::new();
                    let mut root_image: String = String::new();

                    for tok in rest.split_whitespace() {
                        if let Some((k, v)) = tok.split_once('=') {
                            match k {
                                "type" => dep_type = v.to_string(),
                                "predicate" => {
                                    if let Ok(f) = crate::fmri::Fmri::parse(v) {
                                        dep_predicate = Some(f);
                                    }
                                }
                                "fmri" => {
                                    if let Ok(f) = crate::fmri::Fmri::parse(v) {
                                        dep_fmris.push(f);
                                    }
                                }
                                "root-image" => {
                                    root_image = v.to_string();
                                }
                                _ => { /* ignore other props for catalog */ }
                            }
                        }
                    }

                    // For each fmri property, add a Dependency entry
                    for f in dep_fmris {
                        let mut d = crate::actions::Dependency::default();
                        d.fmri = Some(f);
                        d.dependency_type = dep_type.clone();
                        d.predicate = dep_predicate.clone();
                        d.root_image = root_image.clone();
                        manifest.dependencies.push(d);
                    }
                }
            }
        }

        // Ensure the manifest has the correct FMRI attribute
        // Create a Version object from the version string
        let version = if !version_entry.version.is_empty() {
            match crate::fmri::Version::parse(&version_entry.version) {
                Ok(v) => Some(v),
                Err(e) => {
                    // Map the FmriError to a CatalogError
                    return Err(CatalogError::Repository(
                        crate::repository::RepositoryError::Other(format!(
                            "Invalid version format: {}",
                            e
                        )),
                    ));
                }
            }
        } else {
            None
        };

        // Create the FMRI with publisher, stem, and version
        let fmri = Fmri::with_publisher(publisher, stem, version);
        self.ensure_fmri_attribute(&mut manifest, &fmri);

        Ok(manifest)
    }

    /// Ensure the manifest has the correct FMRI attribute
    fn ensure_fmri_attribute(&self, manifest: &mut Manifest, fmri: &Fmri) {
        // Check if the manifest already has an FMRI attribute
        let has_fmri = manifest
            .attributes
            .iter()
            .any(|attr| attr.key == "pkg.fmri");

        // If not, add it
        if !has_fmri {
            let mut attr = crate::actions::Attr::default();
            attr.key = "pkg.fmri".to_string();
            attr.values = vec![fmri.to_string()];
            manifest.attributes.push(attr);
        }
    }

    /// Check if a package is obsolete
    fn is_package_obsolete(&self, manifest: &Manifest) -> bool {
        manifest.attributes.iter().any(|attr| {
            attr.key == "pkg.obsolete" && attr.values.get(0).map_or(false, |v| v == "true")
        })
    }

    /// Query the catalog for packages matching a pattern
    pub fn query_packages(&self, pattern: Option<&str>) -> Result<Vec<PackageInfo>> {
        // Open the catalog database
        let db_cat = Database::open(&self.db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open catalog database: {}", e))
        })?;
        // Begin a read transaction
        let tx_cat = db_cat
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Open the catalog table
        let catalog_table = tx_cat
            .open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open catalog table: {}", e)))?;

        // Open the obsoleted database
        let db_obs = Database::open(&self.obsoleted_db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted database: {}", e))
        })?;
        let tx_obs = db_obs
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        let obsoleted_table = tx_obs.open_table(OBSOLETED_TABLE).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted table: {}", e))
        })?;

        let mut results = Vec::new();

        // Process the catalog table (non-obsolete packages)
        // Iterate through all entries in the table
        for entry_result in catalog_table.iter().map_err(|e| {
            CatalogError::Database(format!("Failed to iterate catalog table: {}", e))
        })? {
            let (key, value) = entry_result.map_err(|e| {
                CatalogError::Database(format!("Failed to get entry from catalog table: {}", e))
            })?;
            let key_str = key.value();

            // Skip if the key doesn't match the pattern
            if let Some(pattern) = pattern {
                if !key_str.contains(pattern) {
                    continue;
                }
            }

            // Parse the key to get stem and version
            let parts: Vec<&str> = key_str.split('@').collect();
            if parts.len() != 2 {
                warn!("Invalid key format: {}", key_str);
                continue;
            }

            let stem = parts[0];
            let version = parts[1];

            // Deserialize the manifest
            let manifest: Manifest = decode_manifest_bytes(value.value())?;

            // Extract the publisher from the FMRI attribute
            let publisher = manifest
                .attributes
                .iter()
                .find(|attr| attr.key == "pkg.fmri")
                .map(|attr| {
                    if let Some(fmri_str) = attr.values.get(0) {
                        // Parse the FMRI string
                        match Fmri::parse(fmri_str) {
                            Ok(fmri) => fmri.publisher.unwrap_or_else(|| "unknown".to_string()),
                            Err(_) => "unknown".to_string(),
                        }
                    } else {
                        "unknown".to_string()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());

            // Create a Version object from the version string
            let version_obj = if !version.is_empty() {
                match crate::fmri::Version::parse(version) {
                    Ok(v) => Some(v),
                    Err(_) => None,
                }
            } else {
                None
            };

            // Create the FMRI with publisher, stem, and version
            let fmri = Fmri::with_publisher(&publisher, stem, version_obj);

            // Add to results (non-obsolete)
            results.push(PackageInfo {
                fmri,
                obsolete: false,
                publisher,
            });
        }

        // Process the obsoleted table (obsolete packages)
        // Iterate through all entries in the table
        for entry_result in obsoleted_table.iter().map_err(|e| {
            CatalogError::Database(format!("Failed to iterate obsoleted table: {}", e))
        })? {
            let (key, _) = entry_result.map_err(|e| {
                CatalogError::Database(format!("Failed to get entry from obsoleted table: {}", e))
            })?;
            let key_str = key.value();

            // Skip if the key doesn't match the pattern
            if let Some(pattern) = pattern {
                if !key_str.contains(pattern) {
                    continue;
                }
            }

            // Parse the key to get the FMRI
            match Fmri::parse(key_str) {
                Ok(fmri) => {
                    // Extract the publisher
                    let publisher = fmri
                        .publisher
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());

                    // Add to results (obsolete)
                    results.push(PackageInfo {
                        fmri,
                        obsolete: true,
                        publisher,
                    });
                }
                Err(e) => {
                    warn!(
                        "Failed to parse FMRI from obsoleted table key: {}: {}",
                        key_str, e
                    );
                    continue;
                }
            }
        }

        Ok(results)
    }

    /// Get a manifest from the catalog
    pub fn get_manifest(&self, fmri: &Fmri) -> Result<Option<Manifest>> {
        // Open the catalog database
        let db_cat = Database::open(&self.db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open catalog database: {}", e))
        })?;
        // Begin a read transaction
        let tx_cat = db_cat
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Open the catalog table
        let catalog_table = tx_cat
            .open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open catalog table: {}", e)))?;

        // Create the key for the catalog table (stem@version)
        let catalog_key = format!("{}@{}", fmri.stem(), fmri.version());

        // Try to get the manifest from the catalog table
        if let Ok(Some(bytes)) = catalog_table.get(catalog_key.as_str()) {
            return Ok(Some(decode_manifest_bytes(bytes.value())?));
        }

        // If not found in catalog DB, check obsoleted DB
        let db_obs = Database::open(&self.obsoleted_db_path).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted database: {}", e))
        })?;
        let tx_obs = db_obs
            .begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        let obsoleted_table = tx_obs.open_table(OBSOLETED_TABLE).map_err(|e| {
            CatalogError::Database(format!("Failed to open obsoleted table: {}", e))
        })?;
        let obsoleted_key = fmri.to_string();
        if let Ok(Some(_)) = obsoleted_table.get(obsoleted_key.as_str()) {
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
        Ok(None)
    }
}
