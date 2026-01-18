use crate::actions::Manifest;
use crate::fmri::Fmri;
use miette::Diagnostic;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;
use tracing::info;

/// Table definition for the installed packages database
/// Key: full FMRI including publisher (pkg://publisher/stem@version)
/// Value: serialized Manifest
pub const INSTALLED_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("installed");

/// Errors that can occur when working with the installed packages database
#[derive(Error, Debug, Diagnostic)]
pub enum InstalledError {
    #[error("IO error: {0}")]
    #[diagnostic(code(ips::installed_error::io))]
    IO(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    #[diagnostic(code(ips::installed_error::json))]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    #[diagnostic(code(ips::installed_error::database))]
    Database(String),

    #[error("FMRI error: {0}")]
    #[diagnostic(code(ips::installed_error::fmri))]
    Fmri(#[from] crate::fmri::FmriError),

    #[error("Package not found: {0}")]
    #[diagnostic(code(ips::installed_error::package_not_found))]
    PackageNotFound(String),
}

/// Result type for installed packages operations
pub type Result<T> = std::result::Result<T, InstalledError>;

/// Information about an installed package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackageInfo {
    /// The FMRI of the package
    pub fmri: Fmri,

    /// The publisher of the package
    pub publisher: String,
}

/// The installed packages database
pub struct InstalledPackages {
    /// Path to the installed packages database
    db_path: PathBuf,
}

impl InstalledPackages {
    // Note on borrowing and redb:
    // When using redb, there's a potential borrowing issue when working with transactions and tables.
    // The issue occurs because:
    // 1. Tables borrow from the transaction they were opened from
    // 2. When committing a transaction with tx.commit(), the transaction is moved
    // 3. If a table is still borrowing from the transaction when commit() is called, Rust's borrow checker will prevent the move
    //
    // To fix this issue, we use block scopes {} around table operations to ensure that the table
    // objects are dropped (and their borrows released) before committing the transaction.
    // This pattern is used in all methods that commit transactions after table operations.

    /// Create a new installed packages database
    pub fn new<P: AsRef<Path>>(db_path: P) -> Self {
        InstalledPackages {
            db_path: db_path.as_ref().to_path_buf(),
        }
    }

    /// Dump the contents of the installed table to stdout for debugging
    pub fn dump_installed_table(&self) -> Result<()> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a read transaction
        let tx = db
            .begin_read()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Open the installed table
        match tx.open_table(INSTALLED_TABLE) {
            Ok(table) => {
                let mut count = 0;
                for entry_result in table.iter().map_err(|e| {
                    InstalledError::Database(format!("Failed to iterate installed table: {}", e))
                })? {
                    let (key, value) = entry_result.map_err(|e| {
                        InstalledError::Database(format!(
                            "Failed to get entry from installed table: {}",
                            e
                        ))
                    })?;
                    let key_str = key.value();

                    // Try to deserialize the manifest
                    match serde_json::from_slice::<Manifest>(value.value()) {
                        Ok(manifest) => {
                            // Extract the publisher from the FMRI attribute
                            let publisher = manifest
                                .attributes
                                .iter()
                                .find(|attr| attr.key == "pkg.fmri")
                                .and_then(|attr| attr.values.first().cloned())
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
                println!("Total entries in installed table: {}", count);
                Ok(())
            }
            Err(e) => {
                println!("Error opening installed table: {}", e);
                Err(InstalledError::Database(format!(
                    "Failed to open installed table: {}",
                    e
                )))
            }
        }
    }

    /// Get database statistics
    pub fn get_db_stats(&self) -> Result<()> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a read transaction
        let tx = db
            .begin_read()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Get table statistics
        let mut installed_count = 0;

        // Count installed entries
        if let Ok(table) = tx.open_table(INSTALLED_TABLE) {
            for result in table.iter().map_err(|e| {
                InstalledError::Database(format!("Failed to iterate installed table: {}", e))
            })? {
                let _ = result.map_err(|e| {
                    InstalledError::Database(format!(
                        "Failed to get entry from installed table: {}",
                        e
                    ))
                })?;
                installed_count += 1;
            }
        }

        // Print statistics
        println!("Database path: {}", self.db_path.display());
        println!("Table statistics:");
        println!("  Installed table: {} entries", installed_count);
        println!("Total entries: {}", installed_count);

        Ok(())
    }

    /// Initialize the installed packages database
    pub fn init_db(&self) -> Result<()> {
        // Create a parent directory if it doesn't exist
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Open or create the database
        let db = Database::create(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to create database: {}", e)))?;

        // Create tables
        let tx = db
            .begin_write()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        tx.open_table(INSTALLED_TABLE).map_err(|e| {
            InstalledError::Database(format!("Failed to create installed table: {}", e))
        })?;

        tx.commit().map_err(|e| {
            InstalledError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        Ok(())
    }

    /// Add a package to the installed packages database
    pub fn add_package(&self, fmri: &Fmri, manifest: &Manifest) -> Result<()> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a writing transaction
        let tx = db
            .begin_write()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Create the key (full FMRI including publisher)
        let key = fmri.to_string();

        // Serialize the manifest
        let manifest_bytes = serde_json::to_vec(manifest)?;

        // Use a block scope to ensure the table is dropped before committing the transaction
        {
            // Open the installed table
            let mut installed_table = tx.open_table(INSTALLED_TABLE).map_err(|e| {
                InstalledError::Database(format!("Failed to open installed table: {}", e))
            })?;

            // Insert the package into the installed table
            installed_table
                .insert(key.as_str(), manifest_bytes.as_slice())
                .map_err(|e| {
                    InstalledError::Database(format!(
                        "Failed to insert into installed table: {}",
                        e
                    ))
                })?;

            // The table is dropped at the end of this block, releasing its borrow of tx
        }

        // Commit the transaction
        tx.commit().map_err(|e| {
            InstalledError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        info!("Added package to installed database: {}", key);
        Ok(())
    }

    /// Remove a package from the installed packages database
    pub fn remove_package(&self, fmri: &Fmri) -> Result<()> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a writing transaction
        let tx = db
            .begin_write()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Create the key (full FMRI including publisher)
        let key = fmri.to_string();

        // Use a block scope to ensure the table is dropped before committing the transaction
        {
            // Open the installed table
            let mut installed_table = tx.open_table(INSTALLED_TABLE).map_err(|e| {
                InstalledError::Database(format!("Failed to open installed table: {}", e))
            })?;

            // Check if the package exists
            if let Ok(None) = installed_table.get(key.as_str()) {
                return Err(InstalledError::PackageNotFound(key));
            }

            // Remove the package from the installed table
            installed_table.remove(key.as_str()).map_err(|e| {
                InstalledError::Database(format!("Failed to remove from installed table: {}", e))
            })?;

            // The table is dropped at the end of this block, releasing its borrow of tx
        }

        // Commit the transaction
        tx.commit().map_err(|e| {
            InstalledError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        info!("Removed package from installed database: {}", key);
        Ok(())
    }

    /// Query the installed packages database for packages matching a pattern
    pub fn query_packages(&self, pattern: Option<&str>) -> Result<Vec<InstalledPackageInfo>> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a read transaction
        let tx = db
            .begin_read()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Use a block scope to ensure the table is dropped when no longer needed
        let results = {
            // Open the installed table
            let installed_table = tx.open_table(INSTALLED_TABLE).map_err(|e| {
                InstalledError::Database(format!("Failed to open installed table: {}", e))
            })?;

            let mut results = Vec::new();

            // Process the installed table
            // Iterate through all entries in the table
            for entry_result in installed_table.iter().map_err(|e| {
                InstalledError::Database(format!("Failed to iterate installed table: {}", e))
            })? {
                let (key, _) = entry_result.map_err(|e| {
                    InstalledError::Database(format!(
                        "Failed to get entry from installed table: {}",
                        e
                    ))
                })?;
                let key_str = key.value();

                // Skip if the key doesn't match the pattern
                if let Some(pattern) = pattern {
                    if !key_str.contains(pattern) {
                        continue;
                    }
                }

                // Parse the key to get the FMRI
                let fmri = Fmri::from_str(key_str)?;

                // Get the publisher (handling the Option<String>)
                let publisher = fmri
                    .publisher
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());

                // Add to results
                results.push(InstalledPackageInfo { fmri, publisher });
            }

            results
            // The table is dropped at the end of this block
        };

        Ok(results)
    }

    /// Get a manifest from the installed packages database
    pub fn get_manifest(&self, fmri: &Fmri) -> Result<Option<Manifest>> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a read transaction
        let tx = db
            .begin_read()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Create the key (full FMRI including publisher)
        let key = fmri.to_string();

        // Use a block scope to ensure the table is dropped when no longer needed
        let manifest_option = {
            // Open the installed table
            let installed_table = tx.open_table(INSTALLED_TABLE).map_err(|e| {
                InstalledError::Database(format!("Failed to open installed table: {}", e))
            })?;

            // Try to get the manifest from the installed table
            if let Ok(Some(bytes)) = installed_table.get(key.as_str()) {
                Some(serde_json::from_slice(bytes.value())?)
            } else {
                None
            }
            // The table is dropped at the end of this block
        };

        Ok(manifest_option)
    }

    /// Check if a package is installed
    pub fn is_installed(&self, fmri: &Fmri) -> Result<bool> {
        // Open the database
        let db = Database::open(&self.db_path)
            .map_err(|e| InstalledError::Database(format!("Failed to open database: {}", e)))?;

        // Begin a read transaction
        let tx = db
            .begin_read()
            .map_err(|e| InstalledError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Create the key (full FMRI including publisher)
        let key = fmri.to_string();

        // Use a block scope to ensure the table is dropped when no longer needed
        let is_installed = {
            // Open the installed table
            let installed_table = tx.open_table(INSTALLED_TABLE).map_err(|e| {
                InstalledError::Database(format!("Failed to open installed table: {}", e))
            })?;

            // Check if the package exists
            if let Ok(Some(_)) = installed_table.get(key.as_str()) {
                true
            } else {
                false
            }
            // The table is dropped at the end of this block
        };

        Ok(is_installed)
    }
}
