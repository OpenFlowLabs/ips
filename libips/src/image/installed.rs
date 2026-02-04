use crate::actions::Manifest;
use crate::fmri::Fmri;
use crate::repository::sqlite_catalog::INSTALLED_SCHEMA;
use miette::Diagnostic;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;
use tracing::info;

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

impl From<rusqlite::Error> for InstalledError {
    fn from(e: rusqlite::Error) -> Self {
        InstalledError::Database(format!("SQLite error: {}", e))
    }
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
    /// Create a new installed packages database
    pub fn new<P: AsRef<Path>>(db_path: P) -> Self {
        InstalledPackages {
            db_path: db_path.as_ref().to_path_buf(),
        }
    }

    /// Dump the contents of the installed table to stdout for debugging
    pub fn dump_installed_table(&self) -> Result<()> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let mut stmt = conn.prepare("SELECT fmri, manifest FROM installed")?;
        let mut rows = stmt.query([])?;

        let mut count = 0;
        while let Some(row) = rows.next()? {
            let fmri_str: String = row.get(0)?;
            let manifest_bytes: Vec<u8> = row.get(1)?;

            match serde_json::from_slice::<Manifest>(&manifest_bytes) {
                Ok(manifest) => {
                    println!("FMRI: {}", fmri_str);
                    println!("  Attributes: {}", manifest.attributes.len());
                    println!("  Files: {}", manifest.files.len());
                    println!("  Directories: {}", manifest.directories.len());
                    println!("  Dependencies: {}", manifest.dependencies.len());
                }
                Err(e) => {
                    println!("FMRI: {}", fmri_str);
                    println!("  Error deserializing manifest: {}", e);
                }
            }
            count += 1;
        }
        println!("Total entries in installed table: {}", count);
        Ok(())
    }

    /// Get database statistics
    pub fn get_db_stats(&self) -> Result<()> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let installed_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM installed", [], |row| row.get(0))?;

        println!("Database path: {}", self.db_path.display());
        println!("Table statistics:");
        println!("  Installed table: {} entries", installed_count);
        println!("Total entries: {}", installed_count);

        Ok(())
    }

    /// Initialize the installed packages database
    pub fn init_db(&self) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create or open the database
        let conn = Connection::open(&self.db_path)?;

        // Execute schema
        conn.execute_batch(INSTALLED_SCHEMA)?;

        Ok(())
    }

    /// Add a package to the installed packages database
    pub fn add_package(&self, fmri: &Fmri, manifest: &Manifest) -> Result<()> {
        let mut conn = Connection::open(&self.db_path)?;

        let key = fmri.to_string();
        let manifest_bytes = serde_json::to_vec(manifest)?;

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO installed (fmri, manifest) VALUES (?1, ?2)",
            rusqlite::params![key, manifest_bytes],
        )?;
        tx.commit()?;

        info!("Added package to installed database: {}", key);
        Ok(())
    }

    /// Remove a package from the installed packages database
    pub fn remove_package(&self, fmri: &Fmri) -> Result<()> {
        let mut conn = Connection::open(&self.db_path)?;

        let key = fmri.to_string();

        // Check if the package exists
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM installed WHERE fmri = ?1)",
            rusqlite::params![key],
            |row| row.get(0),
        )?;

        if !exists {
            return Err(InstalledError::PackageNotFound(key));
        }

        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM installed WHERE fmri = ?1",
            rusqlite::params![key],
        )?;
        tx.commit()?;

        info!("Removed package from installed database: {}", key);
        Ok(())
    }

    /// Query the installed packages database for packages matching a pattern
    pub fn query_packages(&self, pattern: Option<&str>) -> Result<Vec<InstalledPackageInfo>> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let query = if let Some(pattern) = pattern {
            format!(
                "SELECT fmri FROM installed WHERE fmri LIKE '%{}%'",
                pattern.replace('\'', "''")
            )
        } else {
            "SELECT fmri FROM installed".to_string()
        };

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query([])?;

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let fmri_str: String = row.get(0)?;
            let fmri = Fmri::from_str(&fmri_str)?;
            let publisher = fmri
                .publisher
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            results.push(InstalledPackageInfo { fmri, publisher });
        }

        Ok(results)
    }

    /// Get a manifest from the installed packages database
    pub fn get_manifest(&self, fmri: &Fmri) -> Result<Option<Manifest>> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let key = fmri.to_string();
        let result = conn.query_row(
            "SELECT manifest FROM installed WHERE fmri = ?1",
            rusqlite::params![key],
            |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytes)
            },
        );

        match result {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Check if a package is installed
    pub fn is_installed(&self, fmri: &Fmri) -> Result<bool> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let key = fmri.to_string();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM installed WHERE fmri = ?1)",
            rusqlite::params![key],
            |row| row.get(0),
        )?;

        Ok(exists)
    }
}
