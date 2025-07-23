//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{Result, anyhow};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::str::FromStr;
use sha2::{Sha256, Digest as Sha2Digest};
use std::fs::File;
use flate2::write::GzEncoder;
use flate2::Compression as GzipCompression;
use lz4::EncoderBuilder;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use serde::{Serialize, Deserialize};

use crate::actions::{Manifest, File as FileAction};
use crate::digest::Digest;
use crate::fmri::Fmri;
use crate::payload::{Payload, PayloadCompressionAlgorithm};

use super::{Repository, RepositoryConfig, RepositoryVersion, REPOSITORY_CONFIG_FILENAME, PublisherInfo, RepositoryInfo, PackageInfo, PackageContents};

/// Search index for a repository
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearchIndex {
    /// Maps search terms to package FMRIs
    terms: HashMap<String, HashSet<String>>,
    /// Maps package FMRIs to package names
    packages: HashMap<String, String>,
    /// Last updated timestamp
    updated: u64,
}

impl SearchIndex {
    /// Create a new empty search index
    fn new() -> Self {
        SearchIndex {
            terms: HashMap::new(),
            packages: HashMap::new(),
            updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
    
    /// Add a term to the index for a package
    fn add_term(&mut self, term: &str, fmri: &str, name: &str) {
        // Convert term to lowercase for case-insensitive search
        let term = term.to_lowercase();
        
        // Add the term to the index
        self.terms.entry(term)
            .or_insert_with(HashSet::new)
            .insert(fmri.to_string());
        
        // Add the package to the packages map
        self.packages.insert(fmri.to_string(), name.to_string());
    }
    
    /// Add a package to the index
    fn add_package(&mut self, package: &PackageInfo, contents: Option<&PackageContents>) {
        // Get the FMRI as a string
        let fmri = package.fmri.to_string();
        
        // Add the package name as a term
        self.add_term(package.fmri.stem(), &fmri, package.fmri.stem());
        
        // Add the publisher as a term if available
        if let Some(publisher) = &package.fmri.publisher {
            self.add_term(publisher, &fmri, package.fmri.stem());
        }
        
        // Add the version as a term if available
        let version = package.fmri.version();
        if !version.is_empty() {
            self.add_term(&version, &fmri, &package.fmri.stem());
        }
        
        // Add contents if available
        if let Some(content) = contents {
            // Add files
            if let Some(files) = &content.files {
                for file in files {
                    self.add_term(file, &fmri, package.fmri.stem());
                }
            }
            
            // Add directories
            if let Some(directories) = &content.directories {
                for dir in directories {
                    self.add_term(dir, &fmri, package.fmri.stem());
                }
            }
            
            // Add dependencies
            if let Some(dependencies) = &content.dependencies {
                for dep in dependencies {
                    self.add_term(dep, &fmri, package.fmri.stem());
                }
            }
        }
        
        // Update the timestamp
        self.updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
    
    /// Search the index for packages matching a query
    fn search(&self, query: &str, limit: Option<usize>) -> Vec<String> {
        // Convert query to lowercase for case-insensitive search
        let query = query.to_lowercase();
        
        // Split the query into terms
        let terms: Vec<&str> = query.split_whitespace().collect();
        
        // If no terms, return empty result
        if terms.is_empty() {
            return Vec::new();
        }
        
        // Find packages that match all terms
        let mut result_set: Option<HashSet<String>> = None;
        
        for term in terms {
            // Find packages that match this term
            if let Some(packages) = self.terms.get(term) {
                // If this is the first term, initialize the result set
                if result_set.is_none() {
                    result_set = Some(packages.clone());
                } else {
                    // Otherwise, intersect with the current result set
                    result_set = result_set.map(|rs| {
                        rs.intersection(packages)
                            .cloned()
                            .collect()
                    });
                }
            } else {
                // If any term has no matches, the result is empty
                return Vec::new();
            }
        }
        
        // Convert the result set to a vector
        let mut results: Vec<String> = result_set
            .unwrap_or_default()
            .into_iter()
            .collect();
        
        // Sort the results
        results.sort();
        
        // Apply limit if specified
        if let Some(max_results) = limit {
            results.truncate(max_results);
        }
        
        results
    }
    
    /// Save the index to a file
    fn save(&self, path: &Path) -> Result<()> {
        // Create the parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Serialize the index to JSON
        let json = serde_json::to_string(self)?;
        
        // Write the JSON to the file
        fs::write(path, json)?;
        
        Ok(())
    }
    
    /// Load the index from a file
    fn load(path: &Path) -> Result<Self> {
        // Read the file
        let json = fs::read_to_string(path)?;
        
        // Deserialize the JSON
        let index: SearchIndex = serde_json::from_str(&json)?;
        
        Ok(index)
    }
}

/// Repository implementation that uses the local filesystem
pub struct FileBackend {
    pub path: PathBuf,
    pub config: RepositoryConfig,
    /// Catalog manager for handling catalog operations
    catalog_manager: Option<crate::repository::catalog::CatalogManager>,
}

/// Format a SystemTime as an ISO 8601 timestamp string
fn format_iso8601_timestamp(time: &SystemTime) -> String {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    
    let secs = duration.as_secs();
    let micros = duration.subsec_micros();
    
    // Format as ISO 8601 with microsecond precision
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z", 
        // Convert seconds to date and time components
        1970 + secs / 31536000, // year (approximate)
        (secs % 31536000) / 2592000 + 1, // month (approximate)
        (secs % 2592000) / 86400 + 1, // day (approximate)
        (secs % 86400) / 3600, // hour
        (secs % 3600) / 60, // minute
        secs % 60, // second
        micros // microseconds
    )
}

/// Transaction for publishing packages
pub struct Transaction {
    /// Unique ID for the transaction
    id: String,
    /// Path to the transaction directory
    path: PathBuf,
    /// Manifest being updated
    manifest: Manifest,
    /// Files to be published
    files: Vec<(PathBuf, String)>, // (source_path, sha256)
    /// Repository reference
    repo: PathBuf,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(repo_path: PathBuf) -> Result<Self> {
        // Generate a unique ID based on timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let id = format!("trans_{}", timestamp);
        
        // Create transaction directory
        let trans_path = repo_path.join("trans").join(&id);
        fs::create_dir_all(&trans_path)?;
        
        Ok(Transaction {
            id,
            path: trans_path,
            manifest: Manifest::new(),
            files: Vec::new(),
            repo: repo_path,
        })
    }
    
    /// Update the manifest in the transaction
    ///
    /// This intelligently merges the provided manifest with the existing one,
    /// preserving file actions that have already been added to the transaction.
    /// 
    /// The merge strategy:
    /// - Keeps all file actions from the transaction's manifest (these have been processed with checksums, etc.)
    /// - Adds any file actions from the provided manifest that don't exist in the transaction's manifest
    /// - Merges other types of actions (attributes, directories, dependencies, licenses, links) from both manifests
    pub fn update_manifest(&mut self, manifest: Manifest) {
        // Keep track of file paths that are already in the transaction's manifest
        let existing_file_paths: std::collections::HashSet<String> = self.manifest.files
            .iter()
            .map(|f| f.path.clone())
            .collect();
        
        // Add file actions from the provided manifest that don't exist in the transaction's manifest
        for file in manifest.files {
            if !existing_file_paths.contains(&file.path) {
                self.manifest.add_file(file);
            }
        }
        
        // Merge other types of actions
        self.manifest.attributes.extend(manifest.attributes);
        self.manifest.directories.extend(manifest.directories);
        self.manifest.dependencies.extend(manifest.dependencies);
        self.manifest.licenses.extend(manifest.licenses);
        self.manifest.links.extend(manifest.links);
    }
    
    /// Process a file for the transaction
    ///
    /// Takes a FileAction and a path to a file in a prototype directory.
    /// Calculates the file's checksum, compresses the content using the specified algorithm (Gzip or LZ4),
    /// stores the compressed content in a temp file in the transactions directory,
    /// and updates the FileAction with the hash information for both uncompressed and compressed versions.
    pub fn add_file(&mut self, file_action: FileAction, file_path: &Path) -> Result<()> {
        // Calculate SHA256 hash of the file (uncompressed)
        let hash = Self::calculate_file_hash(file_path)?;
        
        // Create a temp file path in the transactions directory
        let temp_file_name = format!("temp_{}", hash);
        let temp_file_path = self.path.join(temp_file_name);
        
        // Check if the temp file already exists
        if temp_file_path.exists() {
            // If it exists, remove it to avoid any issues with existing content
            fs::remove_file(&temp_file_path).map_err(|e| anyhow!("Failed to remove existing temp file: {}", e))?;
        }
        
        // Read the file content
        let file_content = fs::read(file_path).map_err(|e| anyhow!("Failed to read file {}: {}", file_path.display(), e))?;
        
        // Create a payload with the hash information if it doesn't exist
        let mut updated_file_action = file_action;
        let mut payload = updated_file_action.payload.unwrap_or_else(Payload::default);
        
        // Set the compression algorithm (use the one from payload or default to Gzip)
        let compression_algorithm = payload.compression_algorithm;
        
        // Compress the file based on the selected algorithm
        let compressed_hash = match compression_algorithm {
            PayloadCompressionAlgorithm::Gzip => {
                // Create a Gzip encoder with default compression level
                let mut encoder = GzEncoder::new(Vec::new(), GzipCompression::default());
                
                // Write the file content to the encoder
                encoder.write_all(&file_content)
                    .map_err(|e| anyhow!("Failed to write data to Gzip encoder: {}", e))?;
                
                // Finish the compression and get the compressed data
                let compressed_data = encoder.finish()
                    .map_err(|e| anyhow!("Failed to finish Gzip compression: {}", e))?;
                
                // Write the compressed data to the temp file
                fs::write(&temp_file_path, &compressed_data)
                    .map_err(|e| anyhow!("Failed to write compressed data to temp file: {}", e))?;
                
                // Calculate hash of the compressed data
                let mut hasher = Sha256::new();
                hasher.update(&compressed_data);
                format!("{:x}", hasher.finalize())
            },
            PayloadCompressionAlgorithm::LZ4 => {
                // Create an LZ4 encoder with default compression level
                let mut encoder = EncoderBuilder::new().build(Vec::new())
                    .map_err(|e| anyhow!("Failed to create LZ4 encoder: {}", e))?;
                
                // Write the file content to the encoder
                encoder.write_all(&file_content)
                    .map_err(|e| anyhow!("Failed to write data to LZ4 encoder: {}", e))?;
                
                // Finish the compression and get the compressed data
                let (compressed_data, _) = encoder.finish();
                
                // Write the compressed data to the temp file
                fs::write(&temp_file_path, &compressed_data)
                    .map_err(|e| anyhow!("Failed to write LZ4 compressed data to temp file: {}", e))?;
                
                // Calculate hash of the compressed data
                let mut hasher = Sha256::new();
                hasher.update(&compressed_data);
                format!("{:x}", hasher.finalize())
            }
        };
        
        // Add file to the list for later processing during commit
        self.files.push((temp_file_path.clone(), compressed_hash.clone()));
        
        // Set the primary identifier (uncompressed hash)
        payload.primary_identifier = Digest::from_str(&hash)?;
        
        // Set the compression algorithm
        payload.compression_algorithm = compression_algorithm;
        
        // Add the compressed hash as an additional identifier
        let compressed_digest = Digest::from_str(&compressed_hash)?;
        payload.additional_identifiers.push(compressed_digest);
        
        // Update the FileAction with the payload
        updated_file_action.payload = Some(payload);
        
        // Add the FileAction to the manifest
        self.manifest.add_file(updated_file_action);
        
        Ok(())
    }
    
    /// Commit the transaction
    pub fn commit(self) -> Result<()> {
        // Save the manifest to the transaction directory
        let manifest_path = self.path.join("manifest");
        
        // Serialize the manifest to JSON
        let manifest_json = serde_json::to_string_pretty(&self.manifest)?;
        fs::write(&manifest_path, manifest_json)?;
        
        // Copy files to their final location
        for (source_path, hash) in self.files {
            // Create the destination path in the files directory
            let dest_path = self.repo.join("file").join(&hash);
            
            // Copy the file if it doesn't already exist
            if !dest_path.exists() {
                fs::copy(source_path, &dest_path)?;
            }
        }
        
        // Generate a timestamp for the manifest version
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Move the manifest to its final location in the repository
        // Store in both the pkg directory and the trans directory as required
        let pkg_manifest_path = self.repo.join("pkg").join("manifest");
        let trans_manifest_path = self.repo.join("trans").join(format!("manifest_{}", timestamp));
        
        // Copy to pkg directory
        fs::copy(&manifest_path, &pkg_manifest_path)?;
        
        // Move to trans directory
        fs::rename(manifest_path, trans_manifest_path)?;
        
        // Clean up the transaction directory (except for the manifest which was moved)
        fs::remove_dir_all(self.path)?;
        
        Ok(())
    }
    
    /// Calculate SHA256 hash of a file
    fn calculate_file_hash(file_path: &Path) -> Result<String> {
        // Open the file
        let mut file = File::open(file_path)?;
        
        // Create a SHA256 hasher
        let mut hasher = Sha256::new();
        
        // Read the file in chunks and update the hasher
        let mut buffer = [0; 1024];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        
        // Get the hash result
        let hash = hasher.finalize();
        
        // Convert to hex string
        let hash_str = format!("{:x}", hash);
        
        Ok(hash_str)
    }
}

impl Repository for FileBackend {
    /// Create a new repository at the specified path
    fn create<P: AsRef<Path>>(path: P, version: RepositoryVersion) -> Result<Self> {
        let path = path.as_ref();
        
        // Create the repository directory if it doesn't exist
        fs::create_dir_all(path)?;
        
        // Create the repository configuration
        let config = RepositoryConfig {
            version,
            ..Default::default()
        };
        
        // Create the repository structure
        let repo = FileBackend {
            path: path.to_path_buf(),
            config,
            catalog_manager: None,
        };
        
        // Create the repository directories
        repo.create_directories()?;
        
        // Save the repository configuration
        repo.save_config()?;
        
        Ok(repo)
    }
    
    /// Open an existing repository
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        // Check if the repository directory exists
        if !path.exists() {
            return Err(anyhow!("Repository does not exist: {}", path.display()));
        }
        
        // Load the repository configuration
        let config_path = path.join(REPOSITORY_CONFIG_FILENAME);
        let config_data = fs::read_to_string(config_path)?;
        let config: RepositoryConfig = serde_json::from_str(&config_data)?;
        
        Ok(FileBackend {
            path: path.to_path_buf(),
            config,
            catalog_manager: None,
        })
    }
    
    /// Save the repository configuration
    fn save_config(&self) -> Result<()> {
        let config_path = self.path.join(REPOSITORY_CONFIG_FILENAME);
        let config_data = serde_json::to_string_pretty(&self.config)?;
        fs::write(config_path, config_data)?;
        Ok(())
    }
    
    /// Add a publisher to the repository
    fn add_publisher(&mut self, publisher: &str) -> Result<()> {
        if !self.config.publishers.contains(&publisher.to_string()) {
            self.config.publishers.push(publisher.to_string());
            
            // Create publisher-specific directories
            fs::create_dir_all(self.path.join("catalog").join(publisher))?;
            fs::create_dir_all(self.path.join("pkg").join(publisher))?;
            
            // Set as default publisher if no default publisher is set
            if self.config.default_publisher.is_none() {
                self.config.default_publisher = Some(publisher.to_string());
            }
            
            // Save the updated configuration
            self.save_config()?;
        }
        
        Ok(())
    }
    
    /// Remove a publisher from the repository
    fn remove_publisher(&mut self, publisher: &str, dry_run: bool) -> Result<()> {
        if let Some(pos) = self.config.publishers.iter().position(|p| p == publisher) {
            if !dry_run {
                self.config.publishers.remove(pos);
                
                // Remove publisher-specific directories and their contents recursively
                let catalog_dir = self.path.join("catalog").join(publisher);
                let pkg_dir = self.path.join("pkg").join(publisher);
                
                // Remove the catalog directory if it exists
                if catalog_dir.exists() {
                    fs::remove_dir_all(&catalog_dir)
                        .map_err(|e| anyhow!("Failed to remove catalog directory: {}", e))?;
                }
                
                // Remove the package directory if it exists
                if pkg_dir.exists() {
                    fs::remove_dir_all(&pkg_dir)
                        .map_err(|e| anyhow!("Failed to remove package directory: {}", e))?;
                }
                
                // Save the updated configuration
                self.save_config()?;
            }
        }
        
        Ok(())
    }
    
    /// Get repository information
    fn get_info(&self) -> Result<RepositoryInfo> {
        let mut publishers = Vec::new();
        
        for publisher_name in &self.config.publishers {
            // Count packages by scanning the pkg/<publisher> directory
            let publisher_pkg_dir = self.path.join("pkg").join(publisher_name);
            let mut package_count = 0;
            let mut latest_timestamp = SystemTime::UNIX_EPOCH;
            
            // Check if the publisher directory exists
            if publisher_pkg_dir.exists() {
                // Walk through the directory and count package manifests
                if let Ok(entries) = fs::read_dir(&publisher_pkg_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        
                        // Skip directories, only count files (package manifests)
                        if path.is_file() {
                            package_count += 1;
                            
                            // Update the latest timestamp if this file is newer
                            if let Ok(metadata) = fs::metadata(&path) {
                                if let Ok(modified) = metadata.modified() {
                                    if modified > latest_timestamp {
                                        latest_timestamp = modified;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Status is always "online" for file-based repositories
            let status = "online".to_string();
            
            // Format the timestamp in ISO 8601 format
            let updated = if latest_timestamp == SystemTime::UNIX_EPOCH {
                // If no files were found, use current time
                let now = SystemTime::now();
                format_iso8601_timestamp(&now)
            } else {
                format_iso8601_timestamp(&latest_timestamp)
            };
            
            // Create a PublisherInfo struct and add it to the list
            publishers.push(PublisherInfo {
                name: publisher_name.clone(),
                package_count,
                status,
                updated,
            });
        }
        
        // Create and return a RepositoryInfo struct
        Ok(RepositoryInfo { publishers })
    }
    
    /// Set a repository property
    fn set_property(&mut self, property: &str, value: &str) -> Result<()> {
        self.config.properties.insert(property.to_string(), value.to_string());
        self.save_config()?;
        Ok(())
    }
    
    /// Set a publisher property
    fn set_publisher_property(&mut self, publisher: &str, property: &str, value: &str) -> Result<()> {
        // Check if the publisher exists
        if !self.config.publishers.contains(&publisher.to_string()) {
            return Err(anyhow!("Publisher does not exist: {}", publisher));
        }
        
        // Create the property key in the format "publisher/property"
        let key = format!("{}/{}", publisher, property);
        
        // Set the property
        self.config.properties.insert(key, value.to_string());
        
        // Save the updated configuration
        self.save_config()?;
        
        Ok(())
    }
    
    /// List packages in the repository
    fn list_packages(&self, publisher: Option<&str>, pattern: Option<&str>) -> Result<Vec<PackageInfo>> {
        let mut packages = Vec::new();
        
        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(anyhow!("Publisher does not exist: {}", pub_name));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };
        
        // For each publisher, list packages
        for pub_name in publishers {
            // Get the publisher's package directory
            let publisher_pkg_dir = self.path.join("pkg").join(&pub_name);
            
            // Check if the publisher directory exists
            if publisher_pkg_dir.exists() {
                // Verify that the publisher is in the config
                if !self.config.publishers.contains(&pub_name) {
                    return Err(anyhow!("Publisher directory exists but is not in the repository configuration: {}", pub_name));
                }
                
                // Walk through the directory and collect package manifests
                if let Ok(entries) = fs::read_dir(&publisher_pkg_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        
                        // Skip directories, only process files (package manifests)
                        if path.is_file() {
                            // Parse the manifest file to get real package information
                            match Manifest::parse_file(&path) {
                                Ok(manifest) => {
                                    // Look for the pkg.fmri attribute
                                    for attr in &manifest.attributes {
                                        if attr.key == "pkg.fmri" && !attr.values.is_empty() {
                                            let fmri = &attr.values[0];
                                            
                                            // Parse the FMRI using our Fmri type
                                            match Fmri::parse(fmri) {
                                                Ok(parsed_fmri) => {
                                                    // Filter by pattern if specified
                                                    if let Some(pat) = pattern {
                                                        // Try to compile the pattern as a regex
                                                        match Regex::new(pat) {
                                                            Ok(regex) => {
                                                                // Use regex matching
                                                                if !regex.is_match(parsed_fmri.stem()) {
                                                                    continue;
                                                                }
                                                            },
                                                            Err(err) => {
                                                                // Log the error but fall back to simple string contains
                                                                eprintln!("Error compiling regex pattern '{}': {}", pat, err);
                                                                if !parsed_fmri.stem().contains(pat) {
                                                                    continue;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    
                                                    // If the publisher is not set in the FMRI, use the current publisher
                                                    if parsed_fmri.publisher.is_none() {
                                                        let mut fmri_with_publisher = parsed_fmri.clone();
                                                        fmri_with_publisher.publisher = Some(pub_name.clone());
                                                        
                                                        // Create a PackageInfo struct and add it to the list
                                                        packages.push(PackageInfo {
                                                            fmri: fmri_with_publisher,
                                                        });
                                                    } else {
                                                        // Create a PackageInfo struct and add it to the list
                                                        packages.push(PackageInfo {
                                                            fmri: parsed_fmri.clone(),
                                                        });
                                                    }
                                                    
                                                    // Found the package info, no need to check other attributes
                                                    break;
                                                },
                                                Err(err) => {
                                                    // Log the error but continue processing
                                                    eprintln!("Error parsing FMRI '{}': {}", fmri, err);
                                                }
                                            }
                                        }
                                    }
                                },
                                Err(err) => {
                                    // Log the error but continue processing other files
                                    eprintln!("Error parsing manifest file {}: {}", path.display(), err);
                                }
                            }
                        }
                    }
                }
            }
            // No else clause - we don't return placeholder data anymore
        }
        
        Ok(packages)
    }
    
    /// Show the contents of packages
    fn show_contents(&self, publisher: Option<&str>, pattern: Option<&str>, action_types: Option<&[String]>) -> Result<Vec<PackageContents>> {
        // We don't need to get the list of packages since we'll process the manifests directly
        
        // Use a HashMap to store package information
        let mut packages = std::collections::HashMap::new();
        
        // Define a struct to hold the content vectors for each package
        struct PackageContentVectors {
            files: Vec<String>,
            directories: Vec<String>,
            links: Vec<String>,
            dependencies: Vec<String>,
            licenses: Vec<String>,
        }
        
        impl PackageContentVectors {
            fn new() -> Self {
                Self {
                    files: Vec::new(),
                    directories: Vec::new(),
                    links: Vec::new(),
                    dependencies: Vec::new(),
                    licenses: Vec::new(),
                }
            }
        }
        
        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(anyhow!("Publisher does not exist: {}", pub_name));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };
        
        // For each publisher, process packages
        for pub_name in publishers {
            // Get the publisher's package directory
            let publisher_pkg_dir = self.path.join("pkg").join(&pub_name);
            
            // Check if the publisher directory exists
            if publisher_pkg_dir.exists() {
                // Walk through the directory and collect package manifests
                if let Ok(entries) = fs::read_dir(&publisher_pkg_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        
                        // Skip directories, only process files (package manifests)
                        if path.is_file() {
                            // Parse the manifest file to get package information
                            match Manifest::parse_file(&path) {
                                Ok(manifest) => {
                                    // Look for the pkg.fmri attribute to identify the package
                                    let mut pkg_id = String::new();
                                    
                                    for attr in &manifest.attributes {
                                        if attr.key == "pkg.fmri" && !attr.values.is_empty() {
                                            let fmri = &attr.values[0];
                                            
                                            // Parse the FMRI using our Fmri type
                                            match Fmri::parse(fmri) {
                                                Ok(parsed_fmri) => {
                                                    // Filter by pattern if specified
                                                    if let Some(pat) = pattern {
                                                        // Try to compile the pattern as a regex
                                                        match Regex::new(pat) {
                                                            Ok(regex) => {
                                                                // Use regex matching
                                                                if !regex.is_match(parsed_fmri.stem()) {
                                                                    continue;
                                                                }
                                                            },
                                                            Err(err) => {
                                                                // Log the error but fall back to simple string contains
                                                                eprintln!("Error compiling regex pattern '{}': {}", pat, err);
                                                                if !parsed_fmri.stem().contains(pat) {
                                                                    continue;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    
                                                    // Format the package identifier using the FMRI
                                                    let version = parsed_fmri.version();
                                                    pkg_id = if !version.is_empty() {
                                                        format!("{}@{}", parsed_fmri.stem(), version)
                                                    } else {
                                                        parsed_fmri.stem().to_string()
                                                    };
                                                    
                                                    break;
                                                },
                                                Err(err) => {
                                                    // Log the error but continue processing
                                                    eprintln!("Error parsing FMRI '{}': {}", fmri, err);
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Skip if we couldn't determine the package ID
                                    if pkg_id.is_empty() {
                                        continue;
                                    }
                                    
                                    // Get or create the content vectors for this package
                                    let content_vectors = packages
                                        .entry(pkg_id.clone())
                                        .or_insert_with(PackageContentVectors::new);
                                    
                                    // Process file actions
                                    if action_types.is_none() || action_types.as_ref().unwrap().contains(&"file".to_string()) {
                                        for file in &manifest.files {
                                            content_vectors.files.push(file.path.clone());
                                        }
                                    }
                                    
                                    // Process directory actions
                                    if action_types.is_none() || action_types.as_ref().unwrap().contains(&"dir".to_string()) {
                                        for dir in &manifest.directories {
                                            content_vectors.directories.push(dir.path.clone());
                                        }
                                    }
                                    
                                    // Process link actions
                                    if action_types.is_none() || action_types.as_ref().unwrap().contains(&"link".to_string()) {
                                        for link in &manifest.links {
                                            content_vectors.links.push(link.path.clone());
                                        }
                                    }
                                    
                                    // Process dependency actions
                                    if action_types.is_none() || action_types.as_ref().unwrap().contains(&"depend".to_string()) {
                                        for depend in &manifest.dependencies {
                                            if let Some(fmri) = &depend.fmri {
                                                content_vectors.dependencies.push(fmri.to_string());
                                            }
                                        }
                                    }
                                    
                                    // Process license actions
                                    if action_types.is_none() || action_types.as_ref().unwrap().contains(&"license".to_string()) {
                                        for license in &manifest.licenses {
                                            if let Some(path_prop) = license.properties.get("path") {
                                                content_vectors.licenses.push(path_prop.value.clone());
                                            } else if let Some(license_prop) = license.properties.get("license") {
                                                content_vectors.licenses.push(license_prop.value.clone());
                                            } else {
                                                content_vectors.licenses.push(license.payload.clone());
                                            }
                                        }
                                    }
                                },
                                Err(err) => {
                                    // Log the error but continue processing other files
                                    eprintln!("Error parsing manifest file {}: {}", path.display(), err);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Convert the HashMap to a Vec<PackageContents>
        let package_contents = packages
            .into_iter()
            .map(|(package_id, content_vectors)| {
                // Only include non-empty vectors
                let files = if content_vectors.files.is_empty() {
                    None
                } else {
                    Some(content_vectors.files)
                };
                
                let directories = if content_vectors.directories.is_empty() {
                    None
                } else {
                    Some(content_vectors.directories)
                };
                
                let links = if content_vectors.links.is_empty() {
                    None
                } else {
                    Some(content_vectors.links)
                };
                
                let dependencies = if content_vectors.dependencies.is_empty() {
                    None
                } else {
                    Some(content_vectors.dependencies)
                };
                
                let licenses = if content_vectors.licenses.is_empty() {
                    None
                } else {
                    Some(content_vectors.licenses)
                };
                
                PackageContents {
                    package_id,
                    files,
                    directories,
                    links,
                    dependencies,
                    licenses,
                }
            })
            .collect();
        
        Ok(package_contents)
    }
    
    /// Rebuild repository metadata
    fn rebuild(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()> {
        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(anyhow!("Publisher does not exist: {}", pub_name));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };
        
        // For each publisher, rebuild metadata
        for pub_name in publishers {
            println!("Rebuilding metadata for publisher: {}", pub_name);
            
            if !no_catalog {
                println!("Rebuilding catalog...");
                // In a real implementation, we would rebuild the catalog
            }
            
            if !no_index {
                println!("Rebuilding search index...");
                self.build_search_index(&pub_name)?;
            }
        }
        
        Ok(())
    }
    
    /// Refresh repository metadata
    fn refresh(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()> {
        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(anyhow!("Publisher does not exist: {}", pub_name));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };
        
        // For each publisher, refresh metadata
        for pub_name in publishers {
            println!("Refreshing metadata for publisher: {}", pub_name);
            
            if !no_catalog {
                println!("Refreshing catalog...");
                // In a real implementation, we would refresh the catalog
            }
            
            if !no_index {
                println!("Refreshing search index...");
                
                // Check if the index exists
                let index_path = self.path.join("index").join(&pub_name).join("search.json");
                if !index_path.exists() {
                    // If the index doesn't exist, build it
                    self.build_search_index(&pub_name)?;
                } else {
                    // If the index exists, update it
                    // For simplicity, we'll just rebuild it
                    self.build_search_index(&pub_name)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Set the default publisher for the repository
    fn set_default_publisher(&mut self, publisher: &str) -> Result<()> {
        // Check if the publisher exists
        if !self.config.publishers.contains(&publisher.to_string()) {
            return Err(anyhow!("Publisher does not exist: {}", publisher));
        }
        
        // Set the default publisher
        self.config.default_publisher = Some(publisher.to_string());
        
        // Save the updated configuration
        self.save_config()?;
        
        Ok(())
    }
    
    /// Search for packages in the repository
    fn search(&self, query: &str, publisher: Option<&str>, limit: Option<usize>) -> Result<Vec<PackageInfo>> {
        // If no publisher is specified, use the default publisher if available
        let publisher = publisher.or_else(|| self.config.default_publisher.as_deref());
        
        // If still no publisher, we need to search all publishers
        let publishers = if let Some(pub_name) = publisher {
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };
        
        let mut results = Vec::new();
        
        // For each publisher, search the index
        for pub_name in publishers {
            // Check if the index exists
            if let Ok(Some(index)) = self.get_search_index(&pub_name) {
                // Search the index
                let fmris = index.search(query, limit);
                
                // Convert FMRIs to PackageInfo
                for fmri_str in fmris {
                    if let Ok(fmri) = Fmri::parse(&fmri_str) {
                        results.push(PackageInfo { fmri });
                    }
                }
            } else {
                // If the index doesn't exist, fall back to the simple search
                let all_packages = self.list_packages(Some(&pub_name), None)?;
                
                // Filter packages by the query string
                let matching_packages: Vec<PackageInfo> = all_packages
                    .into_iter()
                    .filter(|pkg| {
                        // Match against package name
                        pkg.fmri.stem().contains(query)
                    })
                    .collect();
                
                // Add matching packages to the results
                results.extend(matching_packages);
            }
        }
        
        // Apply limit if specified
        if let Some(max_results) = limit {
            results.truncate(max_results);
        }
        
        Ok(results)
    }
}

impl FileBackend {
    /// Create the repository directories
    fn create_directories(&self) -> Result<()> {
        // Create the main repository directories
        fs::create_dir_all(self.path.join("catalog"))?;
        fs::create_dir_all(self.path.join("file"))?;
        fs::create_dir_all(self.path.join("index"))?;
        fs::create_dir_all(self.path.join("pkg"))?;
        fs::create_dir_all(self.path.join("trans"))?;
        
        Ok(())
    }
    
    /// Get or initialize the catalog manager
    fn get_catalog_manager(&mut self) -> Result<&mut crate::repository::catalog::CatalogManager> {
        if self.catalog_manager.is_none() {
            let catalog_dir = self.path.join("catalog");
            self.catalog_manager = Some(crate::repository::catalog::CatalogManager::new(&catalog_dir)?);
        }
        
        Ok(self.catalog_manager.as_mut().unwrap())
    }
    
    /// URL encode a string for use in a filename
    fn url_encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                ' ' => result.push('+'),
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", c as u8));
                }
            }
        }
        result
    }
    
    /// Generate catalog parts for a publisher
    fn generate_catalog_parts(&mut self, publisher: &str, create_update_log: bool) -> Result<()> {
        println!("Generating catalog parts for publisher: {}", publisher);
        
        // Collect package data first
        let repo_path = self.path.clone();
        let packages = self.list_packages(Some(publisher), None)?;
        
        // Prepare data structures for catalog parts
        let mut base_entries = Vec::new();
        let mut dependency_entries = Vec::new();
        let mut summary_entries = Vec::new();
        let mut update_entries = Vec::new();
        
        // Track package counts
        let mut package_count = 0;
        let mut package_version_count = 0;
        
        // Process each package
        for package in packages {
            let fmri = &package.fmri;
            let stem = fmri.stem();
            
            // Skip if no version
            if fmri.version().is_empty() {
                continue;
            }
            
            // Get the package manifest
            let pkg_dir = repo_path.join("pkg").join(publisher).join(stem);
            if !pkg_dir.exists() {
                continue;
            }
            
            // Get the package version
            let version = fmri.version();
            let encoded_version = Self::url_encode(&version);
            let manifest_path = pkg_dir.join(encoded_version);
            
            if !manifest_path.exists() {
                continue;
            }
            
            // Read the manifest
            let manifest_content = std::fs::read_to_string(&manifest_path)?;
            let manifest = crate::actions::Manifest::parse_string(manifest_content.clone())?;
            
            // Calculate SHA-256 hash of the manifest (as a substitute for SHA-1)
            let mut hasher = sha2::Sha256::new();
            hasher.update(manifest_content.as_bytes());
            let signature = format!("{:x}", hasher.finalize());
            
            // Add to base entries
            base_entries.push((fmri.clone(), None, signature.clone()));
            
            // Extract dependency actions
            let mut dependency_actions = Vec::new();
            for dep in &manifest.dependencies {
                if let Some(dep_fmri) = &dep.fmri {
                    dependency_actions.push(format!("depend fmri={} type={}", dep_fmri, dep.dependency_type));
                }
            }
            
            // Extract variant and facet actions
            for attr in &manifest.attributes {
                if attr.key.starts_with("variant.") || attr.key.starts_with("facet.") {
                    let values_str = attr.values.join(" value=");
                    dependency_actions.push(format!("set name={} value={}", attr.key, values_str));
                }
            }
            
            // Add to dependency entries if there are dependency actions
            if !dependency_actions.is_empty() {
                dependency_entries.push((fmri.clone(), Some(dependency_actions.clone()), signature.clone()));
            }
            
            // Extract summary actions (set actions excluding variants and facets)
            let mut summary_actions = Vec::new();
            for attr in &manifest.attributes {
                if !attr.key.starts_with("variant.") && !attr.key.starts_with("facet.") {
                    let values_str = attr.values.join(" value=");
                    summary_actions.push(format!("set name={} value={}", attr.key, values_str));
                }
            }
            
            // Add to summary entries if there are summary actions
            if !summary_actions.is_empty() {
                summary_entries.push((fmri.clone(), Some(summary_actions.clone()), signature.clone()));
            }
            
            // Prepare update entry if needed
            if create_update_log {
                let mut catalog_parts = std::collections::HashMap::new();
                
                // Add dependency actions to update entry
                if !dependency_actions.is_empty() {
                    let mut actions = std::collections::HashMap::new();
                    actions.insert("actions".to_string(), dependency_actions);
                    catalog_parts.insert("catalog.dependency.C".to_string(), actions);
                }
                
                // Add summary actions to update entry
                if !summary_actions.is_empty() {
                    let mut actions = std::collections::HashMap::new();
                    actions.insert("actions".to_string(), summary_actions);
                    catalog_parts.insert("catalog.summary.C".to_string(), actions);
                }
                
                // Add to update entries
                update_entries.push((fmri.clone(), catalog_parts, signature));
            }
            
            // Update counts
            package_count += 1;
            package_version_count += 1;
        }
        
        // Now get the catalog manager and create the catalog parts
        let catalog_manager = self.get_catalog_manager()?;
        
        // Create and populate the base part
        let base_part_name = "catalog.base.C".to_string();
        let base_part = catalog_manager.create_part(&base_part_name);
        for (fmri, actions, signature) in base_entries {
            base_part.add_package(publisher, &fmri, actions, Some(signature));
        }
        catalog_manager.save_part(&base_part_name)?;
        
        // Create and populate dependency part
        let dependency_part_name = "catalog.dependency.C".to_string();
        let dependency_part = catalog_manager.create_part(&dependency_part_name);
        for (fmri, actions, signature) in dependency_entries {
            dependency_part.add_package(publisher, &fmri, actions, Some(signature));
        }
        catalog_manager.save_part(&dependency_part_name)?;
        
        // Create and populate summary part
        let summary_part_name = "catalog.summary.C".to_string();
        let summary_part = catalog_manager.create_part(&summary_part_name);
        for (fmri, actions, signature) in summary_entries {
            summary_part.add_package(publisher, &fmri, actions, Some(signature));
        }
        catalog_manager.save_part(&summary_part_name)?;
        
        // Create and populate the update log if needed
        if create_update_log {
            let now = std::time::SystemTime::now();
            let timestamp = format_iso8601_timestamp(&now);
            let update_log_name = format!("update.{}Z.C", timestamp.split('.').next().unwrap());
            
            let update_log = catalog_manager.create_update_log(&update_log_name);
            for (fmri, catalog_parts, signature) in update_entries {
                update_log.add_update(
                    publisher,
                    &fmri,
                    crate::repository::catalog::CatalogOperationType::Add,
                    catalog_parts,
                    Some(signature),
                );
            }
            catalog_manager.save_update_log(&update_log_name)?;
        }
        
        // Update catalog attributes
        let now = std::time::SystemTime::now();
        let timestamp = format_iso8601_timestamp(&now);
        
        let attrs = catalog_manager.attrs_mut();
        attrs.last_modified = timestamp.clone();
        attrs.package_count = package_count;
        attrs.package_version_count = package_version_count;
        
        // Add part information
        attrs.parts.insert(base_part_name.clone(), crate::repository::catalog::CatalogPartInfo {
            last_modified: timestamp.clone(),
            signature_sha1: None,
        });
        
        attrs.parts.insert(dependency_part_name.clone(), crate::repository::catalog::CatalogPartInfo {
            last_modified: timestamp.clone(),
            signature_sha1: None,
        });
        
        attrs.parts.insert(summary_part_name.clone(), crate::repository::catalog::CatalogPartInfo {
            last_modified: timestamp.clone(),
            signature_sha1: None,
        });
        
        // Save catalog attributes
        catalog_manager.save_attrs()?;
        
        Ok(())
    }
    
    /// Build a search index for a publisher
    fn build_search_index(&self, publisher: &str) -> Result<()> {
        println!("Building search index for publisher: {}", publisher);
        
        // Create a new search index
        let mut index = SearchIndex::new();
        
        // Get the publisher's package directory
        let publisher_pkg_dir = self.path.join("pkg").join(publisher);
        
        // Check if the publisher directory exists
        if publisher_pkg_dir.exists() {
            // Walk through the directory and process package manifests
            if let Ok(entries) = fs::read_dir(&publisher_pkg_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    
                    // Skip directories, only process files (package manifests)
                    if path.is_file() {
                        // Parse the manifest file to get package information
                        match Manifest::parse_file(&path) {
                            Ok(manifest) => {
                                // Look for the pkg.fmri attribute
                                for attr in &manifest.attributes {
                                    if attr.key == "pkg.fmri" && !attr.values.is_empty() {
                                        let fmri_str = &attr.values[0];
                                        
                                        // Parse the FMRI using our Fmri type
                                        match Fmri::parse(fmri_str) {
                                            Ok(parsed_fmri) => {
                                                // Create a PackageInfo struct
                                                let package_info = PackageInfo {
                                                    fmri: parsed_fmri.clone(),
                                                };
                                                
                                                // Create a PackageContents struct
                                                let version = parsed_fmri.version();
                                                let package_id = if !version.is_empty() {
                                                    format!("{}@{}", parsed_fmri.stem(), version)
                                                } else {
                                                    parsed_fmri.stem().to_string()
                                                };
                                                
                                                // Extract content information
                                                let files = if !manifest.files.is_empty() {
                                                    Some(manifest.files.iter().map(|f| f.path.clone()).collect())
                                                } else {
                                                    None
                                                };
                                                
                                                let directories = if !manifest.directories.is_empty() {
                                                    Some(manifest.directories.iter().map(|d| d.path.clone()).collect())
                                                } else {
                                                    None
                                                };
                                                
                                                let links = if !manifest.links.is_empty() {
                                                    Some(manifest.links.iter().map(|l| l.path.clone()).collect())
                                                } else {
                                                    None
                                                };
                                                
                                                let dependencies = if !manifest.dependencies.is_empty() {
                                                    Some(manifest.dependencies.iter()
                                                        .filter_map(|d| d.fmri.as_ref().map(|f| f.to_string()))
                                                        .collect())
                                                } else {
                                                    None
                                                };
                                                
                                                let licenses = if !manifest.licenses.is_empty() {
                                                    Some(manifest.licenses.iter().map(|l| {
                                                        if let Some(path_prop) = l.properties.get("path") {
                                                            path_prop.value.clone()
                                                        } else if let Some(license_prop) = l.properties.get("license") {
                                                            license_prop.value.clone()
                                                        } else {
                                                            l.payload.clone()
                                                        }
                                                    }).collect())
                                                } else {
                                                    None
                                                };
                                                
                                                let package_contents = PackageContents {
                                                    package_id,
                                                    files,
                                                    directories,
                                                    links,
                                                    dependencies,
                                                    licenses,
                                                };
                                                
                                                // Add the package to the index
                                                index.add_package(&package_info, Some(&package_contents));
                                                
                                                // Found the package info, no need to check other attributes
                                                break;
                                            },
                                            Err(err) => {
                                                // Log the error but continue processing
                                                eprintln!("Error parsing FMRI '{}': {}", fmri_str, err);
                                            }
                                        }
                                    }
                                }
                            },
                            Err(err) => {
                                // Log the error but continue processing other files
                                eprintln!("Error parsing manifest file {}: {}", path.display(), err);
                            }
                        }
                    }
                }
            }
        }
        
        // Save the index to a file
        let index_path = self.path.join("index").join(publisher).join("search.json");
        index.save(&index_path)?;
        
        println!("Search index built for publisher: {}", publisher);
        
        Ok(())
    }
    
    /// Get the search index for a publisher
    fn get_search_index(&self, publisher: &str) -> Result<Option<SearchIndex>> {
        let index_path = self.path.join("index").join(publisher).join("search.json");
        
        if index_path.exists() {
            Ok(Some(SearchIndex::load(&index_path)?))
        } else {
            Ok(None)
        }
    }
    
    #[cfg(test)]
    pub fn test_publish_files(&mut self, test_dir: &Path) -> Result<()> {
        println!("Testing file publishing...");
        
        // Create a test publisher
        self.add_publisher("test")?;
        
        // Create a nested directory structure
        let nested_dir = test_dir.join("nested").join("dir");
        fs::create_dir_all(&nested_dir)?;
        
        // Create a test file in the nested directory
        let test_file_path = nested_dir.join("test_file.txt");
        fs::write(&test_file_path, "This is a test file")?;
        
        // Begin a transaction
        let mut transaction = self.begin_transaction()?;
        
        // Create a FileAction from the test file path
        let mut file_action = FileAction::read_from_path(&test_file_path)?;
        
        // Calculate the relative path from the test file path to the base directory
        let relative_path = test_file_path.strip_prefix(test_dir)?.to_string_lossy().to_string();
        
        // Set the relative path in the FileAction
        file_action.path = relative_path;
        
        // Add the test file to the transaction
        transaction.add_file(file_action, &test_file_path)?;
        
        // Verify that the path in the FileAction is the relative path
        // The path should be "nested/dir/test_file.txt", not the full path
        let expected_path = "nested/dir/test_file.txt";
        let actual_path = &transaction.manifest.files[0].path;
        
        if actual_path != expected_path {
            return Err(anyhow!("Path in FileAction is incorrect. Expected: {}, Actual: {}", 
                              expected_path, actual_path));
        }
        
        // Commit the transaction
        transaction.commit()?;
        
        // Verify the file was stored
        let hash = Transaction::calculate_file_hash(&test_file_path)?;
        let stored_file_path = self.path.join("file").join(&hash);
        
        if !stored_file_path.exists() {
            return Err(anyhow!("File was not stored correctly"));
        }
        
        // Verify the manifest was updated
        let manifest_path = self.path.join("pkg").join("manifest");
        
        if !manifest_path.exists() {
            return Err(anyhow!("Manifest was not created"));
        }
        
        println!("File publishing test passed!");
        
        Ok(())
    }
    
    /// Begin a new transaction for publishing
    pub fn begin_transaction(&self) -> Result<Transaction> {
        Transaction::new(self.path.clone())
    }
    
    /// Publish files from a prototype directory
    pub fn publish_files<P: AsRef<Path>>(&self, proto_dir: P, publisher: &str) -> Result<()> {
        let proto_dir = proto_dir.as_ref();
        
        // Check if the prototype directory exists
        if !proto_dir.exists() {
            return Err(anyhow!("Prototype directory does not exist: {}", proto_dir.display()));
        }
        
        // Check if the publisher exists
        if !self.config.publishers.contains(&publisher.to_string()) {
            return Err(anyhow!("Publisher does not exist: {}", publisher));
        }
        
        // Begin a transaction
        let mut transaction = self.begin_transaction()?;
        
        // Walk the prototype directory and add files to the transaction
        self.add_files_to_transaction(&mut transaction, proto_dir, proto_dir)?;
        
        // Commit the transaction
        transaction.commit()?;
        
        Ok(())
    }
    
    /// Add files from a directory to a transaction
    fn add_files_to_transaction(&self, transaction: &mut Transaction, base_dir: &Path, dir: &Path) -> Result<()> {
        // Read the directory entries
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                // Recursively add files from subdirectories
                self.add_files_to_transaction(transaction, base_dir, &path)?;
            } else {
                // Create a FileAction from the file path
                let mut file_action = FileAction::read_from_path(&path)?;
                
                // Calculate the relative path from the file path to the base directory
                let relative_path = path.strip_prefix(base_dir)?.to_string_lossy().to_string();
                
                // Set the relative path in the FileAction
                file_action.path = relative_path;
                
                // Add the file to the transaction
                transaction.add_file(file_action, &path)?;
            }
        }
        
        Ok(())
    }
    
    /// Store a file in the repository
    pub fn store_file<P: AsRef<Path>>(&self, file_path: P) -> Result<String> {
        let file_path = file_path.as_ref();
        
        // Calculate the SHA256 hash of the file
        let hash = Transaction::calculate_file_hash(file_path)?;
        
        // Create the destination path in the files directory
        let dest_path = self.path.join("file").join(&hash);
        
        // Copy the file if it doesn't already exist
        if !dest_path.exists() {
            fs::copy(file_path, &dest_path)?;
        }
        
        Ok(hash)
    }
}