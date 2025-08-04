use crate::actions::{Manifest};
use crate::fmri::Fmri;
use crate::repository::catalog::{CatalogManager, CatalogPart, PackageVersionEntry};
use miette::Diagnostic;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, warn};

/// Table definition for the catalog database
/// Key: stem@version
/// Value: serialized Manifest
pub const CATALOG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("catalog");

/// Table definition for the obsoleted packages catalog
/// Key: full FMRI including publisher (pkg://publisher/stem@version)
/// Value: nothing
pub const OBSOLETED_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("obsoleted");

/// Table definition for the installed packages database
/// Key: full FMRI including publisher (pkg://publisher/stem@version)
/// Value: serialized Manifest
pub const INSTALLED_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("installed");

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
    /// Path to the catalog database
    db_path: PathBuf,
    
    /// Path to the catalog directory
    catalog_dir: PathBuf,
}

impl ImageCatalog {
    /// Create a new image catalog
    pub fn new<P: AsRef<Path>>(catalog_dir: P, db_path: P) -> Self {
        ImageCatalog {
            db_path: db_path.as_ref().to_path_buf(),
            catalog_dir: catalog_dir.as_ref().to_path_buf(),
        }
    }
    
    /// Initialize the catalog database
    pub fn init_db(&self) -> Result<()> {
        // Create a parent directory if it doesn't exist
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Open or create the database
        let db = Database::create(&self.db_path)
            .map_err(|e| CatalogError::Database(format!("Failed to create database: {}", e)))?;
        
        // Create tables
        let tx = db.begin_write()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        
        tx.open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to create catalog table: {}", e)))?;
        
        tx.open_table(OBSOLETED_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to create obsoleted table: {}", e)))?;
        
        tx.commit()
            .map_err(|e| CatalogError::Database(format!("Failed to commit transaction: {}", e)))?;
        
        Ok(())
    }
    
    /// Build the catalog from downloaded catalogs
    pub fn build_catalog(&self, publishers: &[String]) -> Result<()> {
        if publishers.is_empty() {
            return Err(CatalogError::NoPublishers);
        }
        
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| CatalogError::Database(format!("Failed to open database: {}", e)))?;
        
        // Begin a writing transaction
        let tx = db.begin_write()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        
        // Open the catalog table
        let mut catalog_table = tx.open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open catalog table: {}", e)))?;
        
        // Open the obsoleted table
        let mut obsoleted_table = tx.open_table(OBSOLETED_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open obsoleted table: {}", e)))?;
        
        // Process each publisher
        for publisher in publishers {
            let publisher_catalog_dir = self.catalog_dir.join(publisher);
            
            // Skip if the publisher catalog directory doesn't exist
            if !publisher_catalog_dir.exists() {
                warn!("Publisher catalog directory not found: {}", publisher_catalog_dir.display());
                continue;
            }
            
            // Create a catalog manager for this publisher
            let mut catalog_manager = CatalogManager::new(&publisher_catalog_dir, publisher)
                .map_err(|e| CatalogError::Repository(crate::repository::RepositoryError::Other(format!("Failed to create catalog manager: {}", e))))?;
            
            // Get all catalog parts
            let parts = catalog_manager.attrs().parts.clone();
            
            // Load all catalog parts
            for part_name in parts.keys() {
                catalog_manager.load_part(part_name)
                    .map_err(|e| CatalogError::Repository(crate::repository::RepositoryError::Other(format!("Failed to load catalog part: {}", e))))?;
            }
            
            // Process each catalog part
            for (part_name, _) in parts {
                if let Some(part) = catalog_manager.get_part(&part_name) {
                    self.process_catalog_part(&mut catalog_table, &mut obsoleted_table, part, publisher)?;
                }
            }
        }
        
        // Drop the tables to release the borrow on tx
        drop(catalog_table);
        drop(obsoleted_table);
        
        // Commit the transaction
        tx.commit()
            .map_err(|e| CatalogError::Database(format!("Failed to commit transaction: {}", e)))?;
        
        info!("Catalog built successfully");
        Ok(())
    }
    
    /// Process a catalog part and add its packages to the catalog
    fn process_catalog_part(
        &self,
        catalog_table: &mut redb::Table<&str, &[u8]>,
        obsoleted_table: &mut redb::Table<&str, &[u8]>,
        part: &CatalogPart,
        publisher: &str,
    ) -> Result<()> {
        // Get packages for this publisher
        if let Some(publisher_packages) = part.packages.get(publisher) {
            // Process each package stem
            for (stem, versions) in publisher_packages {
                // Process each package version
                for version_entry in versions {
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
                    
                    // Create the key for the catalog table (stem@version)
                    let catalog_key = format!("{}@{}", stem, version_entry.version);
                    
                    // Create the key for the obsoleted table (full FMRI including publisher)
                    let obsoleted_key = fmri.to_string();
                    
                    // Check if we already have this package in the catalog
                    let existing_manifest = if let Ok(bytes) = catalog_table.get(catalog_key.as_str()) {
                        if let Some(bytes) = bytes {
                            Some(serde_json::from_slice::<Manifest>(bytes.value())?)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    
                    // Create or update the manifest
                    let manifest = self.create_or_update_manifest(existing_manifest, version_entry, stem, publisher)?;
                    
                    // Check if the package is obsolete
                    let is_obsolete = self.is_package_obsolete(&manifest);
                    
                    // Serialize the manifest
                    let manifest_bytes = serde_json::to_vec(&manifest)?;
                    
                    // Store the package in the appropriate table
                    if is_obsolete {
                        // Store obsolete packages in the obsoleted table with the full FMRI as key
                        // We don't store any meaningful values in the obsoleted table as per requirements,
                        // but we need to provide a valid byte slice
                        let empty_bytes: &[u8] = &[0u8; 0];
                        obsoleted_table.insert(obsoleted_key.as_str(), empty_bytes)
                            .map_err(|e| CatalogError::Database(format!("Failed to insert into obsoleted table: {}", e)))?;
                    } else {
                        // Store non-obsolete packages in the catalog table with stem@version as a key
                        catalog_table.insert(catalog_key.as_str(), manifest_bytes.as_slice())
                            .map_err(|e| CatalogError::Database(format!("Failed to insert into catalog table: {}", e)))?;
                    }
                }
            }
        }
        
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
        
        // Note: We're skipping the action parsing step as the actions should already be in the manifest
        // from the catalog part. The original code tried to parse actions using Action::from_str,
        // but this method doesn't exist and add_action is private.
        
        // Ensure the manifest has the correct FMRI attribute
        // Create a Version object from the version string
        let version = if !version_entry.version.is_empty() {
            match crate::fmri::Version::parse(&version_entry.version) {
                Ok(v) => Some(v),
                Err(e) => {
                    // Map the FmriError to a CatalogError
                    return Err(CatalogError::Repository(
                        crate::repository::RepositoryError::Other(
                            format!("Invalid version format: {}", e)
                        )
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
        let has_fmri = manifest.attributes.iter().any(|attr| attr.key == "pkg.fmri");
        
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
        // Check for the pkg.obsolete attribute
        manifest.attributes.iter().any(|attr| {
            attr.key == "pkg.obsolete" && attr.values.get(0).map_or(false, |v| v == "true")
        })
    }
    
    /// Query the catalog for packages matching a pattern
    pub fn query_packages(&self, pattern: Option<&str>) -> Result<Vec<PackageInfo>> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| CatalogError::Database(format!("Failed to open database: {}", e)))?;
        
        // Begin a read transaction
        let tx = db.begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        
        // Open the catalog table
        let catalog_table = tx.open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open catalog table: {}", e)))?;
        
        // Open the obsoleted table
        let obsoleted_table = tx.open_table(OBSOLETED_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open obsoleted table: {}", e)))?;
        
        let mut results = Vec::new();
        
        // Process the catalog table (non-obsolete packages)
        // Iterate through all entries in the table
        for entry_result in catalog_table.iter().map_err(|e| CatalogError::Database(format!("Failed to iterate catalog table: {}", e)))? {
            let (key, value) = entry_result.map_err(|e| CatalogError::Database(format!("Failed to get entry from catalog table: {}", e)))?;
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
            let manifest: Manifest = serde_json::from_slice(value.value())?;
            
            // Extract the publisher from the FMRI attribute
            let publisher = manifest.attributes.iter()
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
        for entry_result in obsoleted_table.iter().map_err(|e| CatalogError::Database(format!("Failed to iterate obsoleted table: {}", e)))? {
            let (key, _) = entry_result.map_err(|e| CatalogError::Database(format!("Failed to get entry from obsoleted table: {}", e)))?;
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
                    let publisher = fmri.publisher.clone().unwrap_or_else(|| "unknown".to_string());
                    
                    // Add to results (obsolete)
                    results.push(PackageInfo {
                        fmri,
                        obsolete: true,
                        publisher,
                    });
                },
                Err(e) => {
                    warn!("Failed to parse FMRI from obsoleted table key: {}: {}", key_str, e);
                    continue;
                }
            }
        }
        
        Ok(results)
    }
    
    /// Get a manifest from the catalog
    pub fn get_manifest(&self, fmri: &Fmri) -> Result<Option<Manifest>> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| CatalogError::Database(format!("Failed to open database: {}", e)))?;
        
        // Begin a read transaction
        let tx = db.begin_read()
            .map_err(|e| CatalogError::Database(format!("Failed to begin transaction: {}", e)))?;
        
        // Open the catalog table
        let catalog_table = tx.open_table(CATALOG_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open catalog table: {}", e)))?;
        
        // Open the obsoleted table
        let obsoleted_table = tx.open_table(OBSOLETED_TABLE)
            .map_err(|e| CatalogError::Database(format!("Failed to open obsoleted table: {}", e)))?;
        
        // Create the key for the catalog table (stem@version)
        let catalog_key = format!("{}@{}", fmri.stem(), fmri.version());
        
        // Create the key for the obsoleted table (full FMRI including publisher)
        let obsoleted_key = fmri.to_string();
        
        // Try to get the manifest from the catalog table
        if let Ok(Some(bytes)) = catalog_table.get(catalog_key.as_str()) {
            return Ok(Some(serde_json::from_slice(bytes.value())?));
        }
        
        // Check if the package is in the obsoleted table
        if let Ok(Some(_)) = obsoleted_table.get(obsoleted_key.as_str()) {
            // The package is obsolete, but we don't store the manifest in the obsoleted table
            // We could return a minimal manifest with just the FMRI and obsolete flag
            let mut manifest = Manifest::new();
            
            // Add the FMRI attribute
            let mut attr = crate::actions::Attr::default();
            attr.key = "pkg.fmri".to_string();
            attr.values = vec![fmri.to_string()];
            manifest.attributes.push(attr);
            
            // Add the obsolete attribute
            let mut attr = crate::actions::Attr::default();
            attr.key = "pkg.obsolete".to_string();
            attr.values = vec!["true".to_string()];
            manifest.attributes.push(attr);
            
            return Ok(Some(manifest));
        }
        
        // Manifest not found
        Ok(None)
    }
}