use crate::fmri::Fmri;
use crate::repository::{Result, RepositoryError};
use bincode::{deserialize, serialize};
use miette::Diagnostic;
use regex::Regex;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sha2::Digest;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Format a SystemTime as an ISO 8601 timestamp string
fn format_timestamp(time: &SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));

    let secs = duration.as_secs();
    let micros = duration.subsec_micros();

    // Format as ISO 8601 with microsecond precision
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        // Convert seconds to date and time components
        1970 + secs / 31536000,          // year (approximate)
        (secs % 31536000) / 2592000 + 1, // month (approximate)
        (secs % 2592000) / 86400 + 1,    // day (approximate)
        (secs % 86400) / 3600,           // hour
        (secs % 3600) / 60,              // minute
        secs % 60,                       // second
        micros                           // microseconds
    )
}

/// Represents an obsoleted package in an export file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObsoletedPackageExport {
    /// The publisher of the package
    pub publisher: String,
    /// The FMRI of the package
    pub fmri: String,
    /// The metadata for the package
    pub metadata: ObsoletedPackageMetadata,
    /// The manifest content
    pub manifest: String,
}

/// Represents a collection of obsoleted packages in an export file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObsoletedPackagesExport {
    /// The version of the export format
    pub version: u32,
    /// The date when the export was created
    pub export_date: String,
    /// The packages in the export
    pub packages: Vec<ObsoletedPackageExport>,
}

/// Errors that can occur in obsoleted package operations
#[derive(Debug, Error, Diagnostic)]
pub enum ObsoletedPackageError {
    #[error("obsoleted package not found: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::not_found),
        help("Check that the package exists in the obsoleted packages directory")
    )]
    NotFound(String),

    #[error("failed to read obsoleted package metadata: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::metadata_read),
        help("Check that the metadata file exists and is valid JSON")
    )]
    MetadataReadError(String),

    #[error("failed to read obsoleted package manifest: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::manifest_read),
        help("Check that the manifest file exists and is readable")
    )]
    ManifestReadError(String),

    #[error("failed to parse obsoleted package metadata: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::metadata_parse),
        help("Check that the metadata file contains valid JSON")
    )]
    MetadataParseError(String),

    #[error("failed to parse FMRI: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::fmri_parse),
        help("Check that the FMRI is valid")
    )]
    FmriParseError(String),

    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::io),
        help("Check system resources and permissions")
    )]
    IoError(String),
    
    #[error("failed to remove obsoleted package: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::remove),
        help("Check that the package exists and is not in use")
    )]
    RemoveError(String),
    
    #[error("invalid pagination parameters: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::pagination),
        help("Check that the page number and page size are valid")
    )]
    PaginationError(String),
    
    #[error("search pattern error: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::search),
        help("Check that the search pattern is valid")
    )]
    SearchPatternError(String),
    
    #[error("index error: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::index),
        help("An error occurred with the obsoleted package index")
    )]
    IndexError(String),
    
    #[error("cache error: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::cache),
        help("An error occurred with the obsoleted package cache")
    )]
    CacheError(String),
    
    #[error("database error: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::database),
        help("An error occurred with the obsoleted package database")
    )]
    DatabaseError(String),
    
    #[error("serialization error: {0}")]
    #[diagnostic(
        code(ips::obsoleted_package_error::serialization),
        help("An error occurred while serializing or deserializing data")
    )]
    SerializationError(String),
}

// Implement From for common error types to make error conversion easier
impl From<std::io::Error> for ObsoletedPackageError {
    fn from(err: std::io::Error) -> Self {
        ObsoletedPackageError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for ObsoletedPackageError {
    fn from(err: serde_json::Error) -> Self {
        ObsoletedPackageError::MetadataParseError(err.to_string())
    }
}

impl From<crate::fmri::FmriError> for ObsoletedPackageError {
    fn from(err: crate::fmri::FmriError) -> Self {
        ObsoletedPackageError::FmriParseError(err.to_string())
    }
}

impl From<redb::Error> for ObsoletedPackageError {
    fn from(err: redb::Error) -> Self {
        ObsoletedPackageError::DatabaseError(err.to_string())
    }
}

impl From<redb::DatabaseError> for ObsoletedPackageError {
    fn from(err: redb::DatabaseError) -> Self {
        ObsoletedPackageError::DatabaseError(err.to_string())
    }
}

impl From<redb::TransactionError> for ObsoletedPackageError {
    fn from(err: redb::TransactionError) -> Self {
        ObsoletedPackageError::DatabaseError(err.to_string())
    }
}

impl From<redb::TableError> for ObsoletedPackageError {
    fn from(err: redb::TableError) -> Self {
        ObsoletedPackageError::DatabaseError(err.to_string())
    }
}

impl From<redb::StorageError> for ObsoletedPackageError {
    fn from(err: redb::StorageError) -> Self {
        ObsoletedPackageError::DatabaseError(err.to_string())
    }
}

impl From<redb::CommitError> for ObsoletedPackageError {
    fn from(err: redb::CommitError) -> Self {
        ObsoletedPackageError::DatabaseError(err.to_string())
    }
}

impl From<bincode::Error> for ObsoletedPackageError {
    fn from(err: bincode::Error) -> Self {
        ObsoletedPackageError::SerializationError(err.to_string())
    }
}

// Implement From<ObsoletedPackageError> for RepositoryError to allow conversion
// This makes it easier to use ObsoletedPackageError with the existing Result type
impl From<ObsoletedPackageError> for RepositoryError {
    fn from(err: ObsoletedPackageError) -> Self {
        match err {
            ObsoletedPackageError::NotFound(msg) => RepositoryError::NotFound(msg),
            ObsoletedPackageError::IoError(msg) => RepositoryError::IoError(std::io::Error::new(std::io::ErrorKind::Other, msg)),
            _ => RepositoryError::Other(err.to_string()),
        }
    }
}

/// Represents a paginated result of obsoleted packages
#[derive(Debug, Clone)]
pub struct PaginatedObsoletedPackages {
    /// The list of obsoleted packages for the current page
    pub packages: Vec<Fmri>,
    /// The total number of obsoleted packages
    pub total_count: usize,
    /// The current page number (1-based)
    pub page: usize,
    /// The number of packages per page
    pub page_size: usize,
    /// The total number of pages
    pub total_pages: usize,
}

/// Key used for indexing obsoleted packages
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct ObsoletedPackageKey {
    /// Publisher name
    publisher: String,
    /// Package stem (name)
    stem: String,
    /// Package version
    version: String,
}

impl ObsoletedPackageKey {
    /// Create a new ObsoletedPackageKey from a publisher and FMRI
    fn new(publisher: &str, fmri: &Fmri) -> Self {
        Self {
            publisher: publisher.to_string(),
            stem: fmri.stem().to_string(),
            version: fmri.version().to_string(),
        }
    }
    
    /// Create a new ObsoletedPackageKey from components
    fn from_components(publisher: &str, stem: &str, version: &str) -> Self {
        Self {
            publisher: publisher.to_string(),
            stem: stem.to_string(),
            version: version.to_string(),
        }
    }
}


// Table definitions for the redb database
// Table for mapping FMRI to content hash
static FMRI_TO_HASH_TABLE: TableDefinition<&[u8], &str> = TableDefinition::new("fmri_to_hash");
// Table for mapping content hash to metadata
static HASH_TO_METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("hash_to_metadata");
// Table for mapping content hash to manifest
static HASH_TO_MANIFEST_TABLE: TableDefinition<&str, &str> = TableDefinition::new("hash_to_manifest");

/// Index of obsoleted packages using redb for faster lookups and content-addressable storage
#[derive(Debug)]
struct RedbObsoletedPackageIndex {
    /// The redb database
    db: Database,
    /// Last time the index was accessed
    last_accessed: Instant,
    /// Whether the index is dirty and needs to be rebuilt
    dirty: bool,
    /// Maximum age of the index before it needs to be rebuilt (in seconds)
    max_age: Duration,
}

impl RedbObsoletedPackageIndex {
    /// Create a new RedbObsoletedPackageIndex
    fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let db_path = base_path.as_ref().join("index.redb");
        debug!("Creating redb database at {}", db_path.display());
        
        // Create the database
        let db = Database::create(&db_path)?;
        
        // Create the tables if they don't exist
        let write_txn = db.begin_write()?;
        {
            write_txn.open_table(FMRI_TO_HASH_TABLE)?;
            write_txn.open_table(HASH_TO_METADATA_TABLE)?;
            write_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
        }
        write_txn.commit()?;
        
        Ok(Self {
            db,
            last_accessed: Instant::now(),
            dirty: false,
            max_age: Duration::from_secs(300), // 5 minutes
        })
    }
    
    /// Check if the index is stale and needs to be rebuilt
    fn is_stale(&self) -> bool {
        self.dirty || self.last_accessed.elapsed() > self.max_age
    }
    
    /// Create an empty temporary file-based RedbObsoletedPackageIndex
    /// 
    /// This is used as a fallback when the database creation fails.
    /// It creates a database in a temporary directory that can be used temporarily.
    fn empty() -> Self {
        debug!("Creating empty temporary file-based redb database");
        
        // Create a temporary directory
        let temp_dir = tempfile::tempdir().unwrap_or_else(|e| {
            error!("Failed to create temporary directory: {}", e);
            panic!("Failed to create temporary directory: {}", e);
        });
        
        // Create a database file in the temporary directory
        let db_path = temp_dir.path().join("empty.redb");
        
        // Create the database
        let db = Database::create(&db_path).unwrap_or_else(|e| {
            error!("Failed to create temporary database: {}", e);
            panic!("Failed to create temporary database: {}", e);
        });
        
        // Create the tables
        let write_txn = db.begin_write().unwrap();
        {
            let _ = write_txn.open_table(FMRI_TO_HASH_TABLE).unwrap();
            let _ = write_txn.open_table(HASH_TO_METADATA_TABLE).unwrap();
            let _ = write_txn.open_table(HASH_TO_MANIFEST_TABLE).unwrap();
        }
        write_txn.commit().unwrap();
        
        Self {
            db,
            last_accessed: Instant::now(),
            dirty: false,
            max_age: Duration::from_secs(300), // 5 minutes
        }
    }
    
    /// Open an existing RedbObsoletedPackageIndex
    fn open<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let db_path = base_path.as_ref().join("index.redb");
        debug!("Opening redb database at {}", db_path.display());
        
        // Open the database
        let db = Database::open(&db_path)?;
        
        Ok(Self {
            db,
            last_accessed: Instant::now(),
            dirty: false,
            max_age: Duration::from_secs(300), // 5 minutes
        })
    }
    
    /// Create or open a RedbObsoletedPackageIndex
    fn create_or_open<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let db_path = base_path.as_ref().join("index.redb");
        
        if db_path.exists() {
            Self::open(base_path)
        } else {
            Self::new(base_path)
        }
    }
    
    /// Add an entry to the index
    fn add_entry(&self, key: &ObsoletedPackageKey, metadata: &ObsoletedPackageMetadata, manifest: &str) -> Result<()> {
        // Calculate content hash if not already present
        let content_hash = if metadata.content_hash.is_empty() {
            let mut hasher = sha2::Sha256::new();
            hasher.update(manifest.as_bytes());
            format!("sha256-{:x}", hasher.finalize())
        } else {
            metadata.content_hash.clone()
        };
        
        // Serialize the key and metadata
        let key_bytes = serialize(key)?;
        let metadata_bytes = serialize(metadata)?;
        
        // Begin write transaction
        let write_txn = self.db.begin_write()?;
        {
            // Open the tables
            let mut fmri_to_hash = write_txn.open_table(FMRI_TO_HASH_TABLE)?;
            let mut hash_to_metadata = write_txn.open_table(HASH_TO_METADATA_TABLE)?;
            let mut hash_to_manifest = write_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
            
            // Insert the entries
            fmri_to_hash.insert(key_bytes.as_slice(), content_hash.as_str())?;
            hash_to_metadata.insert(content_hash.as_str(), metadata_bytes.as_slice())?;
            hash_to_manifest.insert(content_hash.as_str(), manifest)?;
        }
        write_txn.commit()?;
        
        Ok(())
    }
    
    /// Remove an entry from the index
    fn remove_entry(&self, key: &ObsoletedPackageKey) -> Result<bool> {
        // Serialize the key
        let key_bytes = serialize(key)?;
        
        // First, check if the key exists and get the content hash
        let content_hash_option = {
            let read_txn = self.db.begin_read()?;
            let fmri_to_hash = read_txn.open_table(FMRI_TO_HASH_TABLE)?;
            
            // Get the hash value and convert it to a string before the transaction is dropped
            let result = match fmri_to_hash.get(key_bytes.as_slice())? {
                Some(hash) => {
                    let hash_str = hash.value().to_string();
                    Some(hash_str)
                },
                None => None,
            };
            
            // Return the result, which doesn't depend on the transaction anymore
            result
        };
        
        // If the key doesn't exist, return early
        if content_hash_option.is_none() {
            return Ok(false);
        }
        
        let content_hash = content_hash_option.unwrap();
        
        // Check if there are any other entries pointing to the same content hash
        let has_other_references = {
            let read_txn = self.db.begin_read()?;
            let fmri_to_hash = read_txn.open_table(FMRI_TO_HASH_TABLE)?;
            
            // Create a vector to store all keys and hashes
            let mut entries = Vec::new();
            let mut iter = fmri_to_hash.iter()?;
            
            // Collect all entries
            while let Some(entry) = iter.next() {
                let (entry_key, hash) = entry?;
                entries.push((entry_key.value().to_vec(), hash.value().to_string()));
            }
            
            // Drop the transaction and iterator before processing the entries
            drop(iter);
            drop(fmri_to_hash);
            drop(read_txn);
            
            // Now check if there are any other entries with the same hash
            let mut has_refs = false;
            for (entry_key, hash) in entries {
                // Skip the key we're removing
                if entry_key != key_bytes.as_slice() && hash == content_hash {
                    has_refs = true;
                    break;
                }
            }
            
            has_refs
        };
        
        // Now perform the actual removal
        let write_txn = self.db.begin_write()?;
        {
            // Remove the entry from fmri_to_hash
            let mut fmri_to_hash = write_txn.open_table(FMRI_TO_HASH_TABLE)?;
            fmri_to_hash.remove(key_bytes.as_slice())?;
            
            // If there are no other references to the content hash, remove the metadata and manifest
            if !has_other_references {
                let mut hash_to_metadata = write_txn.open_table(HASH_TO_METADATA_TABLE)?;
                let mut hash_to_manifest = write_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
                
                hash_to_metadata.remove(content_hash.as_str())?;
                hash_to_manifest.remove(content_hash.as_str())?;
            }
        }
        
        write_txn.commit()?;
        
        Ok(true)
    }
    
    /// Get an entry from the index
    fn get_entry(&self, key: &ObsoletedPackageKey) -> Result<Option<(ObsoletedPackageMetadata, String)>> {
        // Serialize the key
        let key_bytes = serialize(key)?;
        
        // First, get the content hash
        let content_hash = {
            let read_txn = self.db.begin_read()?;
            let fmri_to_hash = read_txn.open_table(FMRI_TO_HASH_TABLE)?;
            
            // Get the hash and convert to a string before the transaction is dropped
            let result = match fmri_to_hash.get(key_bytes.as_slice())? {
                Some(hash) => Some(hash.value().to_string()),
                None => None,
            };
            
            // Return the result, which doesn't depend on the transaction anymore
            result
        };
        
        // If the content hash is not found, return None
        let content_hash = match content_hash {
            Some(hash) => hash,
            None => return Ok(None),
        };
        
        // Now get the metadata and manifest
        let (metadata_bytes, manifest_str) = {
            let read_txn = self.db.begin_read()?;
            let hash_to_metadata = read_txn.open_table(HASH_TO_METADATA_TABLE)?;
            let hash_to_manifest = read_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
            
            // Get the metadata bytes
            let metadata_bytes = match hash_to_metadata.get(content_hash.as_str())? {
                Some(bytes) => bytes.value().to_vec(),
                None => return Ok(None),
            };
            
            // Get the manifest string
            let manifest_str = match hash_to_manifest.get(content_hash.as_str())? {
                Some(manifest) => manifest.value().to_string(),
                None => return Ok(None),
            };
            
            // Return the results, which don't depend on the transaction anymore
            (metadata_bytes, manifest_str)
        };
        
        // Deserialize the metadata
        let metadata: ObsoletedPackageMetadata = deserialize(&metadata_bytes)?;
        
        Ok(Some((metadata, manifest_str)))
    }
    
    /// Get all entries in the index
    fn get_all_entries(&self) -> Result<Vec<(ObsoletedPackageKey, ObsoletedPackageMetadata, String)>> {
        // First, collect all key-hash pairs
        let key_hash_pairs = {
            let read_txn = self.db.begin_read()?;
            let fmri_to_hash = read_txn.open_table(FMRI_TO_HASH_TABLE)?;
            
            let mut pairs = Vec::new();
            let mut iter = fmri_to_hash.iter()?;
            
            while let Some(entry) = iter.next() {
                let (key_bytes, hash) = entry?;
                // Convert to owned types before the transaction is dropped
                let key_data = key_bytes.value().to_vec();
                let hash_str = hash.value().to_string();
                pairs.push((key_data, hash_str));
            }
            
            pairs
        };
        
        let mut entries = Vec::new();
        
        // Process each key-hash pair
        for (key_data, content_hash) in key_hash_pairs {
            // Deserialize the key
            let key: ObsoletedPackageKey = deserialize(&key_data)?;
            
            // Get the metadata and manifest for this content hash
            let (metadata_bytes, manifest_str) = {
                let read_txn = self.db.begin_read()?;
                let hash_to_metadata = read_txn.open_table(HASH_TO_METADATA_TABLE)?;
                let hash_to_manifest = read_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
                
                // Get the metadata bytes
                let metadata_bytes = match hash_to_metadata.get(content_hash.as_str())? {
                    Some(bytes) => bytes.value().to_vec(),
                    None => {
                        // Metadata not found, skip this entry
                        continue;
                    }
                };
                
                // Get the manifest string
                let manifest_str = match hash_to_manifest.get(content_hash.as_str())? {
                    Some(manifest) => manifest.value().to_string(),
                    None => {
                        // Manifest isn't found, skip this entry
                        continue;
                    }
                };
                
                (metadata_bytes, manifest_str)
            };
            
            // Deserialize the metadata
            let metadata: ObsoletedPackageMetadata = deserialize(&metadata_bytes)?;
            
            entries.push((key, metadata, manifest_str));
        }
        
        Ok(entries)
    }
    
    /// Get entries matching a publisher
    fn get_entries_by_publisher(&self, publisher: &str) -> Result<Vec<(ObsoletedPackageKey, ObsoletedPackageMetadata, String)>> {
        // Get all entries and filter by publisher
        // This is more efficient than implementing a separate method with similar logic
        let all_entries = self.get_all_entries()?;
        
        // Filter entries by publisher
        let filtered_entries = all_entries
            .into_iter()
            .filter(|(key, _, _)| key.publisher == publisher)
            .collect();
        
        Ok(filtered_entries)
    }
    
    /// Search for entries matching a pattern
    #[allow(dead_code)]
    fn search_entries(&self, publisher: &str, pattern: &str) -> Result<Vec<(ObsoletedPackageKey, ObsoletedPackageMetadata, String)>> {
        // Get entries for the publisher
        let publisher_entries = self.get_entries_by_publisher(publisher)?;
        
        // Try to compile the pattern as a regex
        let regex_result = Regex::new(pattern);
        
        // Filter entries based on the pattern
        let filtered_entries = match regex_result {
            Ok(regex) => {
                // Filter entries using regex
                publisher_entries
                    .into_iter()
                    .filter(|(key, metadata, _)| {
                        // Match against the FMRI string
                        regex.is_match(&metadata.fmri) ||
                        // Match against the package name
                        regex.is_match(&key.stem)
                    })
                    .collect()
            },
            Err(_) => {
                // If regex compilation fails, fall back to simple substring matching
                publisher_entries
                    .into_iter()
                    .filter(|(key, metadata, _)| {
                        // Match against the FMRI string
                        metadata.fmri.contains(pattern) ||
                        // Match against the package name
                        key.stem.contains(pattern)
                    })
                    .collect()
            }
        };
        
        Ok(filtered_entries)
    }
    
    /// Clear the index
    fn clear(&self) -> Result<()> {
        // Begin a write transaction
        let write_txn = self.db.begin_write()?;
        {
            // Clear all tables by removing all entries
            // Since redb doesn't have a clear() method, we need to iterate and remove each key
            
            // Clear fmri_to_hash table
            {
                let mut fmri_to_hash = write_txn.open_table(FMRI_TO_HASH_TABLE)?;
                let keys_to_remove = {
                    // First collect all keys in a separate scope
                    let read_txn = self.db.begin_read()?;
                    let fmri_to_hash_read = read_txn.open_table(FMRI_TO_HASH_TABLE)?;
                    let mut keys = Vec::new();
                    let mut iter = fmri_to_hash_read.iter()?;
                    while let Some(entry) = iter.next() {
                        let (key, _) = entry?;
                        keys.push(key.value().to_vec());
                    }
                    keys
                };
                
                // Then remove all keys
                for key in keys_to_remove {
                    fmri_to_hash.remove(key.as_slice())?;
                }
            }
            
            // Clear hash_to_metadata table
            {
                let mut hash_to_metadata = write_txn.open_table(HASH_TO_METADATA_TABLE)?;
                let keys_to_remove = {
                    // First collect all keys in a separate scope
                    let read_txn = self.db.begin_read()?;
                    let hash_to_metadata_read = read_txn.open_table(HASH_TO_METADATA_TABLE)?;
                    let mut keys = Vec::new();
                    let mut iter = hash_to_metadata_read.iter()?;
                    while let Some(entry) = iter.next() {
                        let (key, _) = entry?;
                        keys.push(key.value().to_string());
                    }
                    keys
                };
                
                // Then remove all keys
                for key in keys_to_remove {
                    hash_to_metadata.remove(key.as_str())?;
                }
            }
            
            // Clear hash_to_manifest table
            {
                let mut hash_to_manifest = write_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
                let keys_to_remove = {
                    // First collect all keys in a separate scope
                    let read_txn = self.db.begin_read()?;
                    let hash_to_manifest_read = read_txn.open_table(HASH_TO_MANIFEST_TABLE)?;
                    let mut keys = Vec::new();
                    let mut iter = hash_to_manifest_read.iter()?;
                    while let Some(entry) = iter.next() {
                        let (key, _) = entry?;
                        keys.push(key.value().to_string());
                    }
                    keys
                };
                
                // Then remove all keys
                for key in keys_to_remove {
                    hash_to_manifest.remove(key.as_str())?;
                }
            }
        }
        write_txn.commit()?;
        
        Ok(())
    }
    
    /// Get the number of entries in the index
    fn len(&self) -> Result<usize> {
        // Begin a read transaction
        let read_txn = self.db.begin_read()?;
        
        // Open the fmri_to_hash table
        let fmri_to_hash = read_txn.open_table(FMRI_TO_HASH_TABLE)?;
        
        // Count the entries
        let mut count = 0;
        let mut iter = fmri_to_hash.iter()?;
        
        // Iterate through all entries and count them
        while let Some(entry_result) = iter.next() {
            // Just check if the entry exists, we don't need to access its value
            entry_result?;
            count += 1;
        }
        
        // Drop the iterator and table before returning
        drop(iter);
        drop(fmri_to_hash);
        drop(read_txn);
        
        Ok(count)
    }
    
    /// Check if the index is empty
    #[allow(dead_code)]
    fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}



/// Represents metadata for an obsoleted package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsoletedPackageMetadata {
    /// The FMRI of the obsoleted package
    pub fmri: String,
    
    /// The status of the package (always "obsolete")
    pub status: String,
    
    /// The date when the package was obsoleted
    pub obsolescence_date: String,
    
    /// A message explaining why the package was obsoleted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_message: Option<String>,
    
    /// List of FMRIs that replace this package
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obsoleted_by: Option<Vec<String>>,
    
    /// Version of the metadata schema
    pub metadata_version: u32,
    
    /// Hash of the original manifest content
    pub content_hash: String,
}


impl ObsoletedPackageMetadata {
    /// Create a new ObsoletedPackageMetadata instance
    pub fn new(
        fmri: &str,
        content_hash: &str,
        obsoleted_by: Option<Vec<String>>,
        deprecation_message: Option<String>,
    ) -> Self {
        // Get the current time for obsolescence_date
        let now = SystemTime::now();
        let since_epoch = now.duration_since(UNIX_EPOCH).unwrap();
        let obsolescence_date = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            1970 + since_epoch.as_secs() / 31_536_000,
            (since_epoch.as_secs() % 31_536_000) / 2_592_000 + 1,
            ((since_epoch.as_secs() % 2_592_000) / 86_400) + 1,
            (since_epoch.as_secs() % 86_400) / 3600,
            (since_epoch.as_secs() % 3600) / 60,
            since_epoch.as_secs() % 60
        );

        Self {
            fmri: fmri.to_string(),
            status: "obsolete".to_string(),
            obsolescence_date,
            deprecation_message,
            obsoleted_by,
            metadata_version: 1,
            content_hash: content_hash.to_string(),
        }
    }
}

/// Manages obsoleted packages in the repository
pub struct ObsoletedPackageManager {
    /// Base path for obsoleted packages
    base_path: PathBuf,
    /// Index of obsoleted packages for faster lookups using redb
    index: RwLock<RedbObsoletedPackageIndex>,
}

impl ObsoletedPackageManager {
    /// Create a new ObsoletedPackageManager
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        let base_path = repo_path.as_ref().join("obsoleted");
        
        let index = {
            // Create or open the redb-based index
            let redb_index = RedbObsoletedPackageIndex::create_or_open(&base_path).unwrap_or_else(|e| {
                // Log the error and create an empty redb index
                error!("Failed to create or open redb-based index: {}", e);
                RedbObsoletedPackageIndex::empty()
            });
            RwLock::new(redb_index)
        };
        
        Self { 
            base_path,
            index,
        }
    }

    /// Initialize the obsoleted packages directory structure
    pub fn init(&self) -> Result<()> {
        debug!("Initializing obsoleted packages directory: {}", self.base_path.display());
        fs::create_dir_all(&self.base_path)?;
        
        // Initialize the index
        self.build_index()?;
        
        Ok(())
    }
    
    /// Build the index of obsoleted packages
    fn build_index(&self) -> Result<()> {
        debug!("Building index of obsoleted packages");
        
        // Get a write lock on the index
        let index = self.index.write().map_err(|e| ObsoletedPackageError::IndexError(format!(
            "Failed to acquire write lock on index: {}", e
        )))?;
        
        // Clear the index
        let _ = index.clear();
        
        // Check if the base path exists
        if !self.base_path.exists() {
            debug!("Obsoleted packages directory does not exist: {}", self.base_path.display());
            return Ok(());
        }
        
        // Walk through the directory structure to find all obsoleted packages
        for publisher_entry in fs::read_dir(&self.base_path)
            .map_err(|e| ObsoletedPackageError::IoError(format!(
                "Failed to read obsoleted packages directory {}: {}", 
                self.base_path.display(), e
            )))?
        {
            let publisher_entry = publisher_entry.map_err(|e| ObsoletedPackageError::IoError(format!(
                "Failed to read publisher entry: {}", e
            )))?;
            
            let publisher_path = publisher_entry.path();
            if !publisher_path.is_dir() {
                continue;
            }
            
            let publisher = publisher_path.file_name()
                .ok_or_else(|| ObsoletedPackageError::IoError(format!(
                    "Failed to get publisher name from path: {}", 
                    publisher_path.display()
                )))?
                .to_string_lossy()
                .to_string();
                
            debug!("Indexing obsoleted packages for publisher: {}", publisher);
            
            // Walk through the package directories
            for pkg_entry in fs::read_dir(&publisher_path)
                .map_err(|e| ObsoletedPackageError::IoError(format!(
                    "Failed to read publisher directory {}: {}", 
                    publisher_path.display(), e
                )))?
            {
                let pkg_entry = pkg_entry.map_err(|e| ObsoletedPackageError::IoError(format!(
                    "Failed to read package entry: {}", e
                )))?;
                
                let pkg_path = pkg_entry.path();
                if !pkg_path.is_dir() {
                    continue;
                }
                
                let stem = pkg_path.file_name()
                    .ok_or_else(|| ObsoletedPackageError::IoError(format!(
                        "Failed to get package stem from path: {}", 
                        pkg_path.display()
                    )))?
                    .to_string_lossy()
                    .to_string();
                    
                debug!("Indexing obsoleted package: {}", stem);
                
                // Walk through the version files
                for version_entry in fs::read_dir(&pkg_path)
                    .map_err(|e| ObsoletedPackageError::IoError(format!(
                        "Failed to read package directory {}: {}", 
                        pkg_path.display(), e
                    )))?
                {
                    let version_entry = version_entry.map_err(|e| ObsoletedPackageError::IoError(format!(
                        "Failed to read version entry: {}", e
                    )))?;
                    
                    let version_path = version_entry.path();
                    if !version_path.is_file() {
                        continue;
                    }
                    
                    // Check if this is a metadata file
                    if let Some(extension) = version_path.extension() {
                        if extension != "json" {
                            continue;
                        }
                        
                        // Extract the version from the filename
                        let filename = version_path.file_stem()
                            .ok_or_else(|| ObsoletedPackageError::IoError(format!(
                                "Failed to get version from path: {}", 
                                version_path.display()
                            )))?
                            .to_string_lossy()
                            .to_string();
                            
                        // Construct the manifest path
                        let manifest_path = pkg_path.join(format!("{}.manifest", filename));
                        
                        // Get the last modified time of the metadata file
                        let metadata = fs::metadata(&version_path)
                            .map_err(|e| ObsoletedPackageError::IoError(format!(
                                "Failed to get metadata for file {}: {}", 
                                version_path.display(), e
                            )))?;
                            
                        let _last_modified = metadata.modified()
                            .map_err(|e| ObsoletedPackageError::IoError(format!(
                                "Failed to get last modified time for file {}: {}", 
                                version_path.display(), e
                            )))?;
                            
                        // Create an index entry
                        let key = ObsoletedPackageKey::from_components(&publisher, &stem, &filename);
                        
                        // Read the metadata file
                        let metadata_json = fs::read_to_string(&version_path)
                            .map_err(|e| ObsoletedPackageError::IoError(format!(
                                "Failed to read metadata file {}: {}", 
                                version_path.display(), e
                            )))?;
                            
                        // Parse the metadata
                        let metadata: ObsoletedPackageMetadata = serde_json::from_str(&metadata_json)
                            .map_err(|e| ObsoletedPackageError::MetadataParseError(format!(
                                "Failed to parse metadata from {}: {}", 
                                version_path.display(), e
                            )))?;
                            
                        // Read the manifest file
                        let manifest_content = fs::read_to_string(&manifest_path)
                            .map_err(|e| ObsoletedPackageError::ManifestReadError(format!(
                                "Failed to read manifest file {}: {}", 
                                manifest_path.display(), e
                            )))?;
                        
                        // Add the entry to the index
                        index.add_entry(&key, &metadata, &manifest_content)?;
                    }
                }
            }
        }
        
        // Get the count of indexed packages, handling the Result
        match index.len() {
            Ok(count) => debug!("Indexed {} obsoleted packages", count),
            Err(e) => warn!("Failed to get count of indexed packages: {}", e),
        }
        
        Ok(())
    }
    
    /// Ensure the index is fresh, rebuilding it if necessary
    fn ensure_index_is_fresh(&self) -> Result<()> {
        // Get a read lock on the index to check if it's stale
        let is_stale = {
            let index = self.index.read().map_err(|e| ObsoletedPackageError::IndexError(format!(
                "Failed to acquire read lock on index: {}", e
            )))?;
            
            index.is_stale()
        };
        
        // If the index is stale, rebuild it
        if is_stale {
            debug!("Index is stale, rebuilding");
            self.build_index()?;
        }
        
        Ok(())
    }
    
    /// Update an entry in the index
    fn update_index_entry(&self, publisher: &str, fmri: &Fmri, metadata_path: &Path, manifest_path: &Path) -> Result<()> {
        // Get a write lock on the index
        let index = self.index.write().map_err(|e| ObsoletedPackageError::IndexError(format!(
            "Failed to acquire write lock on index: {}", e
        )))?;
        
        // Create the key
        let key = ObsoletedPackageKey::new(publisher, fmri);
        
        // Read the metadata file
        let metadata_json = fs::read_to_string(metadata_path)
            .map_err(|e| ObsoletedPackageError::MetadataReadError(format!(
                "Failed to read metadata file {}: {}", 
                metadata_path.display(), e
            )))?;
            
        // Parse the metadata
        let metadata: ObsoletedPackageMetadata = serde_json::from_str(&metadata_json)
            .map_err(|e| ObsoletedPackageError::MetadataParseError(format!(
                "Failed to parse metadata from {}: {}", 
                metadata_path.display(), e
            )))?;
            
        // Read the manifest file
        let manifest_content = fs::read_to_string(manifest_path)
            .map_err(|e| ObsoletedPackageError::ManifestReadError(format!(
                "Failed to read manifest file {}: {}", 
                manifest_path.display(), e
            )))?;
        
        // Add the entry to the index
        index.add_entry(&key, &metadata, &manifest_content)?;
        
        Ok(())
    }

    /// Store an obsoleted package
    pub fn store_obsoleted_package(
        &self,
        publisher: &str,
        fmri: &Fmri,
        manifest_content: &str,
        obsoleted_by: Option<Vec<String>>,
        deprecation_message: Option<String>,
    ) -> Result<PathBuf> {
        // Create a publisher directory if it doesn't exist
        let publisher_dir = self.base_path.join(publisher);
        fs::create_dir_all(&publisher_dir)?;

        // Calculate content hash
        let mut hasher = sha2::Sha256::new();
        hasher.update(manifest_content.as_bytes());
        let content_hash = format!("sha256-{:x}", hasher.finalize());

        // Create metadata
        let metadata = ObsoletedPackageMetadata::new(
            &fmri.to_string(),
            &content_hash,
            obsoleted_by,
            deprecation_message,
        );

        // Construct path for the obsoleted package
        let stem = fmri.stem();
        let version = fmri.version();
        let pkg_dir = publisher_dir.join(stem);
        fs::create_dir_all(&pkg_dir)?;

        // URL encode the version to use as filename
        let encoded_version = url_encode(&version);
        let metadata_path = pkg_dir.join(format!("{}.json", encoded_version));

        // Write metadata to file
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json)?;

        // Store the original manifest alongside the metadata
        let manifest_path = pkg_dir.join(format!("{}.manifest", encoded_version));
        fs::write(&manifest_path, manifest_content)?;

        // Update the index with this package
        if let Ok(index) = self.index.write() {
            let key = ObsoletedPackageKey::new(publisher, fmri);
            if let Err(e) = index.add_entry(&key, &metadata, manifest_content) {
                warn!("Failed to add package to index: {}", e);
            }
        } else {
            warn!("Failed to acquire write lock on index, package not added to index: {}", fmri);
        }

        info!("Stored obsoleted package: {}", fmri);
        Ok(metadata_path)
    }

    /// Check if a package is obsoleted
    pub fn is_obsoleted(&self, publisher: &str, fmri: &Fmri) -> bool {
        // First check the filesystem directly for faster results in tests
        let stem = fmri.stem();
        let version = fmri.version();
        let encoded_version = url_encode(&version);
        let metadata_path = self.base_path.join(publisher).join(stem).join(format!("{}.json", encoded_version));
        
        if metadata_path.exists() {
            return true;
        }
        
        // Ensure the index is fresh
        if let Err(e) = self.ensure_index_is_fresh() {
            warn!("Failed to ensure index is fresh: {}", e);
            // Already checked the filesystem above, so return false
            return false;
        }
        
        // Check the index
        let key = ObsoletedPackageKey::new(publisher, fmri);
        match self.index.read() {
            Ok(index) => {
                // Properly handle the Result returned by get_entry
                match index.get_entry(&key) {
                    Ok(Some(_)) => true,
                    Ok(None) => false,
                    Err(e) => {
                        warn!("Error checking if package is obsoleted in index: {}", e);
                        false
                    }
                }
            },
            Err(e) => {
                warn!("Failed to acquire read lock on index: {}", e);
                // Already checked the filesystem above, so return false
                false
            }
        }
    }

    /// Get metadata for an obsoleted package
    pub fn get_obsoleted_package_metadata(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<Option<ObsoletedPackageMetadata>> {
        // Ensure the index is fresh
        if let Err(e) = self.ensure_index_is_fresh() {
            warn!("Failed to ensure index is fresh: {}", e);
            // Fall back to the filesystem check if the index is not available
            return self.get_obsoleted_package_metadata_from_filesystem(publisher, fmri);
        }
        
        // Check the index
        let key = ObsoletedPackageKey::new(publisher, fmri);
        
        // Try to get a read lock on the index
        let index_read_result = self.index.read();
        if let Err(e) = index_read_result {
            warn!("Failed to acquire read lock on index: {}", e);
            // Fall back to the filesystem check if the index is not available
            return self.get_obsoleted_package_metadata_from_filesystem(publisher, fmri);
        }
        
        let index = index_read_result.unwrap();
        
        // Check if the package is in the index
        match index.get_entry(&key) {
            Ok(Some((metadata, _))) => {
                // Return the metadata directly from the index
                Ok(Some(metadata))
            },
            Ok(None) => {
                // Package not found in the index, fall back to the filesystem check
                self.get_obsoleted_package_metadata_from_filesystem(publisher, fmri)
            },
            Err(e) => {
                warn!("Failed to get entry from index: {}", e);
                // Fall back to the filesystem check if there's an error
                self.get_obsoleted_package_metadata_from_filesystem(publisher, fmri)
            }
        }
    }
    
    /// Get metadata for an obsoleted package from the filesystem
    fn get_obsoleted_package_metadata_from_filesystem(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<Option<ObsoletedPackageMetadata>> {
        let stem = fmri.stem();
        let version = fmri.version();
        let encoded_version = url_encode(&version);
        let metadata_path = self.base_path.join(publisher).join(stem).join(format!("{}.json", encoded_version));
        let manifest_path = self.base_path.join(publisher).join(stem).join(format!("{}.manifest", encoded_version));

        if !metadata_path.exists() {
            debug!("Metadata file not found: {}", metadata_path.display());
            return Ok(None);
        }

        // Read the metadata file
        let metadata_json = fs::read_to_string(&metadata_path)
            .map_err(|e| ObsoletedPackageError::MetadataReadError(format!(
                "Failed to read metadata file {}: {}", 
                metadata_path.display(), e
            )))?;
        
        // Parse the metadata JSON
        let metadata: ObsoletedPackageMetadata = serde_json::from_str(&metadata_json)
            .map_err(|e| ObsoletedPackageError::MetadataParseError(format!(
                "Failed to parse metadata from {}: {}", 
                metadata_path.display(), e
            )))?;
            
        // Update the index with this package
        if metadata_path.exists() && manifest_path.exists() {
            if let Err(e) = self.update_index_entry(publisher, fmri, &metadata_path, &manifest_path) {
                warn!("Failed to update index entry: {}", e);
            }
        }
            
        Ok(Some(metadata))
    }
    
    /// Get the manifest content for an obsoleted package
    ///
    /// This method retrieves the original manifest content for an obsoleted package.
    /// It can be used to restore the package to the main repository.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher of the obsoleted package
    /// * `fmri` - The FMRI of the obsoleted package
    ///
    /// # Returns
    ///
    /// The manifest content as a string, or None if the package is not found
    pub fn get_obsoleted_package_manifest(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<Option<String>> {
        // Ensure the index is fresh
        if let Err(e) = self.ensure_index_is_fresh() {
            warn!("Failed to ensure index is fresh: {}", e);
            // Fall back to the filesystem check if the index is not available
            return self.get_obsoleted_package_manifest_from_filesystem(publisher, fmri);
        }
        
        // Check the index
        let key = ObsoletedPackageKey::new(publisher, fmri);
        
        // Try to get a read lock on the index
        let index_read_result = self.index.read();
        if let Err(e) = index_read_result {
            warn!("Failed to acquire read lock on index: {}", e);
            // Fall back to the filesystem check if the index is not available
            return self.get_obsoleted_package_manifest_from_filesystem(publisher, fmri);
        }
        
        let index = index_read_result.unwrap();
        
        // Check if the package is in the index
        match index.get_entry(&key) {
            Ok(Some((_, manifest))) => {
                // Return the manifest content directly from the index
                Ok(Some(manifest))
            },
            Ok(None) => {
                // Package not found in the index, fall back to the filesystem check
                self.get_obsoleted_package_manifest_from_filesystem(publisher, fmri)
            },
            Err(e) => {
                warn!("Failed to get entry from index: {}", e);
                // Fall back to the filesystem check if there's an error
                self.get_obsoleted_package_manifest_from_filesystem(publisher, fmri)
            }
        }
    }
    
    /// Get the manifest content for an obsoleted package from the filesystem
    fn get_obsoleted_package_manifest_from_filesystem(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<Option<String>> {
        let stem = fmri.stem();
        let version = fmri.version();
        let encoded_version = url_encode(&version);
        let metadata_path = self.base_path.join(publisher).join(stem).join(format!("{}.json", encoded_version));
        let manifest_path = self.base_path.join(publisher).join(stem).join(format!("{}.manifest", encoded_version));

        if !manifest_path.exists() {
            debug!("Manifest file not found: {}", manifest_path.display());
            return Ok(None);
        }

        // Read the manifest file
        let manifest_content = fs::read_to_string(&manifest_path)
            .map_err(|e| ObsoletedPackageError::ManifestReadError(format!(
                "Failed to read manifest file {}: {}", 
                manifest_path.display(), e
            )))?;
            
        // Update the index with this package
        if metadata_path.exists() && manifest_path.exists() {
            if let Err(e) = self.update_index_entry(publisher, fmri, &metadata_path, &manifest_path) {
                warn!("Failed to update index entry: {}", e);
            }
        }
            
        Ok(Some(manifest_content))
    }
    
    /// Get manifest content and remove an obsoleted package
    ///
    /// This method retrieves the manifest content of an obsoleted package and removes it
    /// from the obsoleted packages directory. It's used as part of the process to restore
    /// an obsoleted package to the main repository.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher of the obsoleted package
    /// * `fmri` - The FMRI of the obsoleted package
    ///
    /// # Returns
    ///
    /// The manifest content if the package was found, or an error if the operation failed
    pub fn get_and_remove_obsoleted_package(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<String> {
        debug!("Getting and removing obsoleted package: {} (publisher: {})", fmri, publisher);
        
        // Get the manifest content
        let manifest_content = match self.get_obsoleted_package_manifest(publisher, fmri)? {
            Some(content) => content,
            None => {
                return Err(ObsoletedPackageError::NotFound(format!(
                    "Obsoleted package not found: {}",
                    fmri
                )).into());
            }
        };
        
        // Remove the obsoleted package from the obsoleted packages directory
        self.remove_obsoleted_package(publisher, fmri)?;
        
        info!("Retrieved and removed obsoleted package: {}", fmri);
        Ok(manifest_content)
    }
    
    /// Remove an obsoleted package
    ///
    /// This method removes an obsoleted package from the obsoleted packages directory.
    /// It can be used after restoring a package to the main repository.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher of the obsoleted package
    /// * `fmri` - The FMRI of the obsoleted package
    ///
    /// # Returns
    ///
    /// `true` if the package was removed, `false` if it was not found
    pub fn remove_obsoleted_package(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<bool> {
        let stem = fmri.stem();
        let version = fmri.version();
        let encoded_version = url_encode(&version);
        let metadata_path = self.base_path.join(publisher).join(stem).join(format!("{}.json", encoded_version));
        let manifest_path = self.base_path.join(publisher).join(stem).join(format!("{}.manifest", encoded_version));
        
        debug!("Removing obsoleted package: {} (publisher: {})", fmri, publisher);
        debug!("Metadata path: {}", metadata_path.display());
        debug!("Manifest path: {}", manifest_path.display());
        
        if !metadata_path.exists() && !manifest_path.exists() {
            // Package not found
            debug!("Obsoleted package not found: {}", fmri);
            return Ok(false);
        }
        
        // Remove the metadata file if it exists
        if metadata_path.exists() {
            debug!("Removing metadata file: {}", metadata_path.display());
            fs::remove_file(&metadata_path)
                .map_err(|e| ObsoletedPackageError::RemoveError(format!(
                    "Failed to remove metadata file {}: {}", 
                    metadata_path.display(), e
                )))?;
        }
        
        // Remove the manifest file if it exists
        if manifest_path.exists() {
            debug!("Removing manifest file: {}", manifest_path.display());
            fs::remove_file(&manifest_path)
                .map_err(|e| ObsoletedPackageError::RemoveError(format!(
                    "Failed to remove manifest file {}: {}", 
                    manifest_path.display(), e
                )))?;
        }
        
        // Check if the package directory is empty and remove it if it is
        let pkg_dir = self.base_path.join(publisher).join(stem);
        if pkg_dir.exists() {
            debug!("Checking if package directory is empty: {}", pkg_dir.display());
            let is_empty = fs::read_dir(&pkg_dir)
                .map_err(|e| ObsoletedPackageError::IoError(format!(
                    "Failed to read directory {}: {}", 
                    pkg_dir.display(), e
                )))?.next().is_none();
                
            if is_empty {
                debug!("Removing empty package directory: {}", pkg_dir.display());
                fs::remove_dir(&pkg_dir)
                    .map_err(|e| ObsoletedPackageError::RemoveError(format!(
                        "Failed to remove directory {}: {}", 
                        pkg_dir.display(), e
                    )))?;
            }
        }
        
        // Remove the package from the index
        let key = ObsoletedPackageKey::new(publisher, fmri);
        
        // Try to get a write lock on the index
        match self.index.write() {
            Ok(index) => {
                // Try to remove the entry from the index
                match index.remove_entry(&key) {
                    Ok(true) => {
                        debug!("Removed package from index: {}", fmri);
                    },
                    Ok(false) => {
                        debug!("Package not found in index: {}", fmri);
                        // If the package is not in the index, we need to rebuild the index
                        // This is a fallback in case the index is out of sync with the filesystem
                        if let Err(e) = self.build_index() {
                            warn!("Failed to rebuild index after package not found: {}", e);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to remove package from index: {}: {}", fmri, e);
                        // If there's an error removing the entry, rebuild the index
                        if let Err(e) = self.build_index() {
                            warn!("Failed to rebuild index after error: {}", e);
                        }
                    }
                }
            },
            Err(e) => {
                warn!("Failed to acquire write lock on index, package not removed from index: {}: {}", fmri, e);
                // If we can't get a write lock, mark the index as dirty so it will be rebuilt next time
                if let Ok(index) = self.index.write() {
                    // This is a new write attempt, so it might succeed even if the previous one failed
                    if let Err(e) = index.clear() {
                        warn!("Failed to clear index: {}", e);
                    }
                }
            }
        }
        
        info!("Removed obsoleted package: {}", fmri);
        Ok(true)
    }

    /// List all obsoleted packages for a publisher
    ///
    /// This method returns all obsoleted packages for a publisher without pagination.
    /// For large repositories, consider using `list_obsoleted_packages_paginated` instead.
    pub fn list_obsoleted_packages(&self, publisher: &str) -> Result<Vec<Fmri>> {
        // Ensure the index is fresh
        if let Err(e) = self.ensure_index_is_fresh() {
            warn!("Failed to ensure index is fresh: {}", e);
            // Fall back to the filesystem check if the index is not available
            return self.list_obsoleted_packages_from_filesystem(publisher);
        }
        
        // Try to get a read lock on the index
        let index_read_result = self.index.read();
        if let Err(e) = index_read_result {
            warn!("Failed to acquire read lock on index: {}", e);
            // Fall back to the filesystem check if the index is not available
            return self.list_obsoleted_packages_from_filesystem(publisher);
        }
        
        let index = index_read_result.unwrap();
        
        // Use get_entries_by_publisher to get all entries for the specified publisher
        let entries = match index.get_entries_by_publisher(publisher) {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to get entries for publisher from index: {}", e);
                // Fall back to the filesystem check if there's an error
                return self.list_obsoleted_packages_from_filesystem(publisher);
            }
        };
        
        // Convert entries to FMRIs
        let mut packages = Vec::new();
        for (key, _, _) in entries {
            // Try to parse the FMRI from the components
            let fmri_str = format!("pkg://{}/{}@{}", key.publisher, key.stem, key.version);
            if let Ok(fmri) = Fmri::parse(&fmri_str) {
                packages.push(fmri);
            }
        }
        
        Ok(packages)
    }
    
    /// List all obsoleted packages for a publisher from the filesystem
    ///
    /// This method is used as a fallback when the index is not available.
    fn list_obsoleted_packages_from_filesystem(&self, publisher: &str) -> Result<Vec<Fmri>> {
        let publisher_dir = self.base_path.join(publisher);
        if !publisher_dir.exists() {
            return Ok(Vec::new());
        }

        let mut obsoleted_packages = Vec::new();

        // Walk through the publisher directory
        for entry in walkdir::WalkDir::new(&publisher_dir)
            .min_depth(2) // Skip the publisher directory itself
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                // Read the metadata file
                if let Ok(metadata_json) = fs::read_to_string(path) {
                    if let Ok(metadata) = serde_json::from_str::<ObsoletedPackageMetadata>(&metadata_json) {
                        // Parse the FMRI
                        if let Ok(fmri) = Fmri::parse(&metadata.fmri) {
                            obsoleted_packages.push(fmri);
                        }
                    }
                }
            }
        }

        Ok(obsoleted_packages)
    }
    

    /// List obsoleted packages for a publisher with pagination
    ///
    /// This method returns a paginated list of obsoleted packages for a publisher.
    /// It's useful when dealing with repositories that have many obsoleted packages.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher to list packages for
    /// * `page` - The page number (1-based, defaults to 1)
    /// * `page_size` - The number of packages per page (defaults to 100)
    ///
    /// # Returns
    ///
    /// A paginated result containing the packages for the requested page
    pub fn list_obsoleted_packages_paginated(
        &self,
        publisher: &str,
        page: Option<usize>,
        page_size: Option<usize>,
    ) -> Result<PaginatedObsoletedPackages> {
        let publisher_dir = self.base_path.join(publisher);
        if !publisher_dir.exists() {
            return Ok(PaginatedObsoletedPackages {
                packages: Vec::new(),
                total_count: 0,
                page: 1,
                page_size: page_size.unwrap_or(100),
                total_pages: 0,
            });
        }

        let mut all_packages = Vec::new();

        // Walk through the publisher directory
        for entry in walkdir::WalkDir::new(&publisher_dir)
            .min_depth(2) // Skip the publisher directory itself
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                // Read the metadata file
                if let Ok(metadata_json) = fs::read_to_string(path) {
                    if let Ok(metadata) = serde_json::from_str::<ObsoletedPackageMetadata>(&metadata_json) {
                        // Parse the FMRI
                        if let Ok(fmri) = Fmri::parse(&metadata.fmri) {
                            all_packages.push(fmri);
                        }
                    }
                }
            }
        }
        
        // Sort packages by name and version for consistent pagination
        all_packages.sort_by(|a, b| {
            let name_cmp = a.stem().cmp(b.stem());
            if name_cmp == std::cmp::Ordering::Equal {
                a.version().cmp(&b.version())
            } else {
                name_cmp
            }
        });

        // Calculate pagination
        let page = page.unwrap_or(1).max(1); // Ensure page is at least 1
        let page_size = page_size.unwrap_or(100);
        let total_count = all_packages.len();
        let total_pages = if total_count == 0 {
            0
        } else {
            (total_count + page_size - 1) / page_size
        };
        
        // If no pagination is requested or there's only one page, return all packages
        if page_size == 0 || total_pages <= 1 {
            return Ok(PaginatedObsoletedPackages {
                packages: all_packages,
                total_count,
                page: 1,
                page_size,
                total_pages,
            });
        }
        
        // Calculate start and end indices for the requested page
        let start_idx = (page - 1) * page_size;
        let end_idx = start_idx + page_size;
        
        // Get packages for the requested page
        let packages = if start_idx >= total_count {
            // If start index is beyond the total count, return an empty page
            Vec::new()
        } else {
            all_packages[start_idx..end_idx.min(total_count)].to_vec()
        };
        
        Ok(PaginatedObsoletedPackages {
            packages,
            total_count,
            page,
            page_size,
            total_pages,
        })
    }
    
    /// Search for obsoleted packages matching a pattern
    ///
    /// This method searches for obsoleted packages that match the given pattern.
    /// The pattern can be a simple substring or a regular expression.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher to search in
    /// * `pattern` - The pattern to search for (substring or regex)
    ///
    /// # Returns
    ///
    /// A list of FMRIs for obsoleted packages that match the pattern
    pub fn search_obsoleted_packages(&self, publisher: &str, pattern: &str) -> Result<Vec<Fmri>> {
        // Ensure the index is fresh
        if let Err(e) = self.ensure_index_is_fresh() {
            warn!("Failed to ensure index is fresh: {}", e);
            // Fall back to the filesystem-based search
            return self.search_obsoleted_packages_fallback(publisher, pattern);
        }
        
        // Try to get a read lock on the index
        let index_read_result = self.index.read();
        if let Err(e) = index_read_result {
            warn!("Failed to acquire read lock on index: {}", e);
            // Fall back to the filesystem-based search
            return self.search_obsoleted_packages_fallback(publisher, pattern);
        }
        
        let index = index_read_result.unwrap();
        
        // Get all entries from the index
        let entries = match index.get_all_entries() {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to get entries from index: {}", e);
                // Fall back to the filesystem-based search
                return self.search_obsoleted_packages_fallback(publisher, pattern);
            }
        };
        
        // Check if the pattern looks like a version number
        if pattern.chars().all(|c| c.is_digit(10) || c == '.') {
            // This looks like a version number, so match only against the version part
            let mut packages = Vec::new();
            for (key, _, _) in entries {
                if key.publisher == publisher && key.version.contains(pattern) {
                    // Construct the FMRI string
                    let fmri_str = format!("pkg://{}/{}@{}", key.publisher, key.stem, key.version);
                    
                    // Parse the FMRI
                    if let Ok(fmri) = Fmri::parse(&fmri_str) {
                        packages.push(fmri);
                    }
                }
            }
            return Ok(packages);
        }
        
        // Try to compile the pattern as a regex
        let result = match Regex::new(pattern) {
            Ok(regex) => {
                // Collect packages from the index that match the regex
                let mut packages = Vec::new();
                for (key, _, _) in entries {
                    if key.publisher == publisher {
                        // Construct the FMRI string for regex matching
                        let fmri_str = format!("pkg://{}/{}@{}", key.publisher, key.stem, key.version);
                        
                        // Match against the FMRI string or the package name
                        if regex.is_match(&fmri_str) || regex.is_match(&key.stem) {
                            // Parse the FMRI
                            if let Ok(fmri) = Fmri::parse(&fmri_str) {
                                packages.push(fmri);
                            }
                        }
                    }
                }
                packages
            },
            Err(_) => {
                // Fall back to simple substring matching
                let mut packages = Vec::new();
                for (key, _, _) in entries {
                    if key.publisher == publisher {
                        // Construct the FMRI string
                        let fmri_str = format!("pkg://{}/{}@{}", key.publisher, key.stem, key.version);
                        
                        // Match against the FMRI string or the package name
                        // For "package-" pattern, we want to match only packages that start with "package-"
                        if pattern.ends_with("-") && key.stem.starts_with(pattern) {
                            // Parse the FMRI
                            if let Ok(fmri) = Fmri::parse(&fmri_str) {
                                packages.push(fmri);
                            }
                        } 
                        // For version searches like "2.0", match only the version part
                        else if pattern.chars().all(|c| c.is_digit(10) || c == '.') {
                            // This looks like a version number, so match only against the version part
                            if key.version.contains(pattern) {
                                // Parse the FMRI
                                if let Ok(fmri) = Fmri::parse(&fmri_str) {
                                    packages.push(fmri);
                                }
                            }
                        } 
                        else if fmri_str.contains(pattern) || key.stem.contains(pattern) {
                            // Parse the FMRI
                            if let Ok(fmri) = Fmri::parse(&fmri_str) {
                                packages.push(fmri);
                            }
                        }
                    }
                }
                packages
            }
        };
        
        Ok(result)
    }
    
    /// Fallback implementation of search_obsoleted_packages that uses the filesystem
    fn search_obsoleted_packages_fallback(&self, publisher: &str, pattern: &str) -> Result<Vec<Fmri>> {
        // Get all obsoleted packages for the publisher
        let all_packages = self.list_obsoleted_packages(publisher)?;
        
        // Check if the pattern looks like a version number
        if pattern.chars().all(|c| c.is_digit(10) || c == '.') {
            // This looks like a version number, so match only against the version part
            return Ok(all_packages
                .into_iter()
                .filter(|fmri| fmri.version().contains(pattern))
                .collect());
        }
        
        // Try to compile the pattern as a regex
        let result = match Regex::new(pattern) {
            Ok(regex) => {
                // Filter packages using regex
                all_packages
                    .into_iter()
                    .filter(|fmri| {
                        // Match against the FMRI string
                        regex.is_match(&fmri.to_string()) ||
                        // Match against the package name
                        regex.is_match(fmri.stem())
                    })
                    .collect()
            },
            Err(_) => {
                // If regex compilation fails, fall back to simple substring matching
                all_packages
                    .into_iter()
                    .filter(|fmri| {
                        // Match against the FMRI string or the package name
                        // For "package-" pattern, we want to match only packages that start with "package-"
                        if pattern.ends_with("-") && fmri.stem().starts_with(pattern) {
                            true
                        } 
                        // For version searches like "2.0", match only the version part
                        else if pattern.chars().all(|c| c.is_digit(10) || c == '.') {
                            // This looks like a version number, so match only against the version part
                            fmri.version().contains(pattern)
                        } 
                        else {
                            // Match against the FMRI string
                            fmri.to_string().contains(pattern) ||
                            // Match against the package name
                            fmri.stem().contains(pattern)
                        }
                    })
                    .collect()
            }
        };
        
        Ok(result)
    }
    
    /// Export obsoleted packages to a file
    ///
    /// This method exports obsoleted packages to a JSON file that can be imported into another repository.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher to export packages for
    /// * `pattern` - Optional pattern to filter packages by
    /// * `output_file` - Path to the output file
    ///
    /// # Returns
    ///
    /// The number of packages exported
    pub fn export_obsoleted_packages(
        &self,
        publisher: &str,
        pattern: Option<&str>,
        output_file: &Path,
    ) -> Result<usize> {
        info!("Exporting obsoleted packages for publisher: {}", publisher);
        
        // Get the packages to export
        let packages = if let Some(pattern) = pattern {
            self.search_obsoleted_packages(publisher, pattern)?
        } else {
            self.list_obsoleted_packages(publisher)?
        };
        
        if packages.is_empty() {
            info!("No packages found to export");
            return Ok(0);
        }
        
        info!("Found {} packages to export", packages.len());
        
        // Create the export structure
        let mut export = ObsoletedPackagesExport {
            version: 1,
            export_date: format_timestamp(&SystemTime::now()),
            packages: Vec::new(),
        };
        
        // Add each package to the export
        for fmri in packages {
            // Get the metadata
            let metadata = match self.get_obsoleted_package_metadata(publisher, &fmri)? {
                Some(metadata) => metadata,
                None => {
                    warn!("Metadata not found for package: {}", fmri);
                    continue;
                }
            };
            
            // Get the manifest content
            let manifest = match self.get_obsoleted_package_manifest(publisher, &fmri)? {
                Some(manifest) => manifest,
                None => {
                    warn!("Manifest not found for package: {}", fmri);
                    continue;
                }
            };
            
            // Add the package to the export
            export.packages.push(ObsoletedPackageExport {
                publisher: publisher.to_string(),
                fmri: fmri.to_string(),
                metadata,
                manifest,
            });
        }
        
        // Write the export to the output file
        let file = fs::File::create(output_file).map_err(|e| ObsoletedPackageError::IoError(format!(
            "Failed to create output file {}: {}", 
            output_file.display(), e
        )))?;
        
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &export).map_err(|e| ObsoletedPackageError::IoError(format!(
            "Failed to write export to file {}: {}", 
            output_file.display(), e
        )))?;
        
        info!("Exported {} packages to {}", export.packages.len(), output_file.display());
        Ok(export.packages.len())
    }
    
    /// Import obsoleted packages from a file
    ///
    /// This method imports obsoleted packages from a JSON file created by `export_obsoleted_packages`.
    ///
    /// # Arguments
    ///
    /// * `input_file` - Path to the input file
    /// * `override_publisher` - Optional publisher to use instead of the one in the export file
    ///
    /// # Returns
    ///
    /// The number of packages imported
    pub fn import_obsoleted_packages(
        &self,
        input_file: &Path,
        override_publisher: Option<&str>,
    ) -> Result<usize> {
        info!("Importing obsoleted packages from {}", input_file.display());
        
        // Read the export file
        let file = fs::File::open(input_file).map_err(|e| ObsoletedPackageError::IoError(format!(
            "Failed to open input file {}: {}", 
            input_file.display(), e
        )))?;
        
        let reader = std::io::BufReader::new(file);
        let export: ObsoletedPackagesExport = serde_json::from_reader(reader).map_err(|e| ObsoletedPackageError::IoError(format!(
            "Failed to parse export from file {}: {}", 
            input_file.display(), e
        )))?;
        
        info!("Found {} packages to import", export.packages.len());
        
        // Import each package
        let mut imported_count = 0;
        for package in export.packages {
            // Determine the publisher to use
            let publisher = override_publisher.unwrap_or(&package.publisher);
            
            // Parse the FMRI
            let fmri = match Fmri::parse(&package.fmri) {
                Ok(fmri) => fmri,
                Err(e) => {
                    warn!("Failed to parse FMRI '{}': {}", package.fmri, e);
                    continue;
                }
            };
            
            // Store the obsoleted package
            match self.store_obsoleted_package(
                publisher,
                &fmri,
                &package.manifest,
                package.metadata.obsoleted_by,
                package.metadata.deprecation_message,
            ) {
                Ok(_) => {
                    info!("Imported obsoleted package: {}", fmri);
                    imported_count += 1;
                },
                Err(e) => {
                    warn!("Failed to import obsoleted package {}: {}", fmri, e);
                }
            }
        }
        
        info!("Imported {} packages", imported_count);
        Ok(imported_count)
    }
    
    /// Batch process multiple obsoleted packages
    ///
    /// This method applies a function to multiple obsoleted packages in batch.
    /// It's useful for operations that need to be performed on many packages at once.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher of the obsoleted packages
    /// * `fmris` - A list of FMRIs to process
    /// * `batch_size` - The number of packages to process in each batch (default: 100)
    /// * `processor` - A function that takes a publisher and an FMRI and returns a result
    ///
    /// # Returns
    ///
    /// A list of results, one for each input FMRI
    pub fn batch_process<F, T, E>(&self, 
        publisher: &str, 
        fmris: &[Fmri], 
        batch_size: Option<usize>,
        processor: F
    ) -> Result<Vec<std::result::Result<T, E>>>
    where
        F: Fn(&str, &Fmri) -> std::result::Result<T, E>,
        E: std::fmt::Debug,
    {
        let batch_size = batch_size.unwrap_or(100);
        let mut results = Vec::with_capacity(fmris.len());
        
        // Process packages in batches
        for chunk in fmris.chunks(batch_size) {
            for fmri in chunk {
                let result = processor(publisher, fmri);
                results.push(result);
            }
        }
        
        Ok(results)
    }
}

/// URL encode a string
fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => encoded.push(c),
            _ => {
                for b in c.to_string().as_bytes() {
                    encoded.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_obsoleted_package_manager_basic() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let manager = ObsoletedPackageManager::new(temp_dir.path());

        // Initialize the manager
        manager.init().unwrap();

        // Create a test FMRI
        let fmri = Fmri::parse("pkg://test/package@1.0,5.11-0.1:20250101T000000Z").unwrap();

        // Store an obsoleted package
        let manifest_content = r#"{
            "attributes": [
                {
                    "key": "pkg.fmri",
                    "values": ["pkg://test/package@1.0,5.11-0.1:20250101T000000Z"]
                },
                {
                    "key": "pkg.obsolete",
                    "values": ["true"]
                }
            ]
        }"#;

        let obsoleted_by = Some(vec!["pkg://test/new-package@2.0".to_string()]);
        let deprecation_message = Some("This package is deprecated. Use new-package instead.".to_string());

        let metadata_path = manager
            .store_obsoleted_package(
                "test",
                &fmri,
                manifest_content,
                obsoleted_by.clone(),
                deprecation_message.clone(),
            )
            .unwrap();

        // Check if the metadata file exists
        assert!(metadata_path.exists());

        // Check if the package is obsoleted
        assert!(manager.is_obsoleted("test", &fmri));

        // Get the metadata
        let metadata = manager.get_obsoleted_package_metadata("test", &fmri).unwrap().unwrap();
        assert_eq!(metadata.fmri, fmri.to_string());
        assert_eq!(metadata.status, "obsolete");
        assert_eq!(metadata.obsoleted_by, obsoleted_by);
        assert_eq!(metadata.deprecation_message, deprecation_message);

        // List obsoleted packages
        let obsoleted_packages = manager.list_obsoleted_packages("test").unwrap();
        assert_eq!(obsoleted_packages.len(), 1);
        assert_eq!(obsoleted_packages[0].to_string(), fmri.to_string());
    }
    
    #[test]
    fn test_obsoleted_package_manager_search() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let manager = ObsoletedPackageManager::new(temp_dir.path());
        manager.init().unwrap();
        
        // Create multiple test FMRIs
        let fmri1 = Fmri::parse("pkg://test/package-one@1.0,5.11-0.1:20250101T000000Z").unwrap();
        let fmri2 = Fmri::parse("pkg://test/package-two@2.0,5.11-0.1:20250101T000000Z").unwrap();
        let fmri3 = Fmri::parse("pkg://test/other-package@3.0,5.11-0.1:20250101T000000Z").unwrap();
        
        // Store obsoleted packages
        let manifest_template = r#"{
            "attributes": [
                {
                    "key": "pkg.fmri",
                    "values": ["%s"]
                },
                {
                    "key": "pkg.obsolete",
                    "values": ["true"]
                }
            ]
        }"#;
        
        let manifest1 = manifest_template.replace("%s", &fmri1.to_string());
        let manifest2 = manifest_template.replace("%s", &fmri2.to_string());
        let manifest3 = manifest_template.replace("%s", &fmri3.to_string());
        
        manager.store_obsoleted_package("test", &fmri1, &manifest1, None, None).unwrap();
        manager.store_obsoleted_package("test", &fmri2, &manifest2, None, None).unwrap();
        manager.store_obsoleted_package("test", &fmri3, &manifest3, None, None).unwrap();
        
        // Test search with substring
        let results = manager.search_obsoleted_packages("test", "package-").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|f| f.to_string() == fmri1.to_string()));
        assert!(results.iter().any(|f| f.to_string() == fmri2.to_string()));
        
        // Test search with regex
        let results = manager.search_obsoleted_packages("test", "package-.*").unwrap();
        assert_eq!(results.len(), 2);
        
        // Test search for specific version
        let results = manager.search_obsoleted_packages("test", "2.0").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].to_string(), fmri2.to_string());
        
        // Test search with no matches
        let results = manager.search_obsoleted_packages("test", "nonexistent").unwrap();
        assert_eq!(results.len(), 0);
    }
    
    #[test]
    fn test_obsoleted_package_manager_pagination() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let manager = ObsoletedPackageManager::new(temp_dir.path());
        manager.init().unwrap();
        
        // Create 10 test FMRIs
        let mut fmris = Vec::new();
        let manifest_template = r#"{
            "attributes": [
                {
                    "key": "pkg.fmri",
                    "values": ["%s"]
                },
                {
                    "key": "pkg.obsolete",
                    "values": ["true"]
                }
            ]
        }"#;
        
        for i in 1..=10 {
            let fmri = Fmri::parse(&format!("pkg://test/package-{:02}@1.0,5.11-0.1:20250101T000000Z", i)).unwrap();
            let manifest = manifest_template.replace("%s", &fmri.to_string());
            manager.store_obsoleted_package("test", &fmri, &manifest, None, None).unwrap();
            fmris.push(fmri);
        }
        
        // Test pagination with page size 3
        let page1 = manager.list_obsoleted_packages_paginated("test", Some(1), Some(3)).unwrap();
        assert_eq!(page1.packages.len(), 3);
        assert_eq!(page1.total_count, 10);
        assert_eq!(page1.page, 1);
        assert_eq!(page1.page_size, 3);
        assert_eq!(page1.total_pages, 4);
        
        let page2 = manager.list_obsoleted_packages_paginated("test", Some(2), Some(3)).unwrap();
        assert_eq!(page2.packages.len(), 3);
        assert_eq!(page2.page, 2);
        
        let page4 = manager.list_obsoleted_packages_paginated("test", Some(4), Some(3)).unwrap();
        assert_eq!(page4.packages.len(), 1); // Last page has only 1 item
        
        // Test pagination with page beyond total
        let empty_page = manager.list_obsoleted_packages_paginated("test", Some(5), Some(3)).unwrap();
        assert_eq!(empty_page.packages.len(), 0);
        assert_eq!(empty_page.total_count, 10);
        assert_eq!(empty_page.page, 5);
        
        // Test with no pagination
        let all_packages = manager.list_obsoleted_packages_paginated("test", None, None).unwrap();
        assert_eq!(all_packages.packages.len(), 10);
    }
    
    #[test]
    fn test_obsoleted_package_manager_remove() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let manager = ObsoletedPackageManager::new(temp_dir.path());
        manager.init().unwrap();
        
        // Create a test FMRI
        let fmri = Fmri::parse("pkg://test/package@1.0,5.11-0.1:20250101T000000Z").unwrap();
        
        // Store an obsoleted package
        let manifest_content = r#"{
            "attributes": [
                {
                    "key": "pkg.fmri",
                    "values": ["pkg://test/package@1.0,5.11-0.1:20250101T000000Z"]
                },
                {
                    "key": "pkg.obsolete",
                    "values": ["true"]
                }
            ]
        }"#;
        
        manager.store_obsoleted_package("test", &fmri, manifest_content, None, None).unwrap();
        
        // Verify the package exists
        assert!(manager.is_obsoleted("test", &fmri));
        
        // Remove the package
        let removed = manager.remove_obsoleted_package("test", &fmri).unwrap();
        assert!(removed);
        
        // Verify the package no longer exists
        assert!(!manager.is_obsoleted("test", &fmri));
        
        // Try to remove a non-existent package
        let not_removed = manager.remove_obsoleted_package("test", &fmri).unwrap();
        assert!(!not_removed);
    }
    
    #[test]
    fn test_obsoleted_package_manager_batch_processing() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let manager = ObsoletedPackageManager::new(temp_dir.path());
        manager.init().unwrap();
        
        // Create multiple test FMRIs
        let fmri1 = Fmri::parse("pkg://test/package-one@1.0,5.11-0.1:20250101T000000Z").unwrap();
        let fmri2 = Fmri::parse("pkg://test/package-two@2.0,5.11-0.1:20250101T000000Z").unwrap();
        let fmri3 = Fmri::parse("pkg://test/package-three@3.0,5.11-0.1:20250101T000000Z").unwrap();
        
        // Store obsoleted packages
        let manifest_template = r#"{
            "attributes": [
                {
                    "key": "pkg.fmri",
                    "values": ["%s"]
                },
                {
                    "key": "pkg.obsolete",
                    "values": ["true"]
                }
            ]
        }"#;
        
        let manifest1 = manifest_template.replace("%s", &fmri1.to_string());
        let manifest2 = manifest_template.replace("%s", &fmri2.to_string());
        let manifest3 = manifest_template.replace("%s", &fmri3.to_string());
        
        manager.store_obsoleted_package("test", &fmri1, &manifest1, None, None).unwrap();
        manager.store_obsoleted_package("test", &fmri2, &manifest2, None, None).unwrap();
        manager.store_obsoleted_package("test", &fmri3, &manifest3, None, None).unwrap();
        
        // Test batch processing with is_obsoleted
        let fmris = vec![fmri1.clone(), fmri2.clone(), fmri3.clone()];
        let results: Vec<std::result::Result<bool, std::convert::Infallible>> = 
            manager.batch_process("test", &fmris, Some(2), |pub_name, fmri| {
                Ok(manager.is_obsoleted(pub_name, fmri))
            }).unwrap();
        
        assert_eq!(results.len(), 3);
        assert!(results[0].as_ref().unwrap());
        assert!(results[1].as_ref().unwrap());
        assert!(results[2].as_ref().unwrap());
        
        // Test batch processing with remove
        let results: Vec<std::result::Result<bool, RepositoryError>> = 
            manager.batch_process("test", &fmris, Some(2), |pub_name, fmri| {
                manager.remove_obsoleted_package(pub_name, fmri)
            }).unwrap();
        
        assert_eq!(results.len(), 3);
        assert!(results[0].as_ref().unwrap());
        assert!(results[1].as_ref().unwrap());
        assert!(results[2].as_ref().unwrap());
        
        // Verify all packages are removed
        assert!(!manager.is_obsoleted("test", &fmri1));
        assert!(!manager.is_obsoleted("test", &fmri2));
        assert!(!manager.is_obsoleted("test", &fmri3));
    }
}