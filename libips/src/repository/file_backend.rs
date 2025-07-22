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

use crate::actions::{Manifest, File as FileAction};
use crate::digest::Digest;
use crate::payload::{Payload, PayloadCompressionAlgorithm};

use super::{Repository, RepositoryConfig, RepositoryVersion, REPOSITORY_CONFIG_FILENAME, PublisherInfo, RepositoryInfo, PackageInfo};

/// Repository implementation that uses the local filesystem
pub struct FileBackend {
    pub path: PathBuf,
    pub config: RepositoryConfig,
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
                                            
                                            // Parse the FMRI to extract package name and version
                                            // Format: pkg://publisher/package_name@version
                                            if let Some(pkg_part) = fmri.strip_prefix("pkg://") {
                                                if let Some(at_pos) = pkg_part.find('@') {
                                                    let pkg_with_pub = &pkg_part[0..at_pos];
                                                    let version = &pkg_part[at_pos+1..];
                                                    
                                                    // Extract package name (may include publisher)
                                                    let pkg_name = if let Some(slash_pos) = pkg_with_pub.find('/') {
                                                        // Skip publisher part if present
                                                        let pub_end = slash_pos + 1;
                                                        &pkg_with_pub[pub_end..]
                                                    } else {
                                                        pkg_with_pub
                                                    };
                                                    
                                                    // Filter by pattern if specified
                                                    if let Some(pat) = pattern {
                                                        // Try to compile the pattern as a regex
                                                        match Regex::new(pat) {
                                                            Ok(regex) => {
                                                                // Use regex matching
                                                                if !regex.is_match(pkg_name) {
                                                                    continue;
                                                                }
                                                            },
                                                            Err(err) => {
                                                                // Log the error but fall back to simple string contains
                                                                eprintln!("Error compiling regex pattern '{}': {}", pat, err);
                                                                if !pkg_name.contains(pat) {
                                                                    continue;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    
                                                    // Create a PackageInfo struct and add it to the list
                                                    packages.push(PackageInfo {
                                                        name: pkg_name.to_string(),
                                                        version: version.to_string(),
                                                        publisher: pub_name.clone(),
                                                    });
                                                    
                                                    // Found the package info, no need to check other attributes
                                                    break;
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
    fn show_contents(&self, publisher: Option<&str>, pattern: Option<&str>, action_types: Option<&[String]>) -> Result<Vec<(String, String, String)>> {
        // This is a placeholder implementation
        // In a real implementation, we would parse package manifests and extract contents
        
        // Get the list of packages
        let packages = self.list_packages(publisher, pattern)?;
        
        // For each package, list contents
        let mut contents = Vec::new();
        
        for pkg_info in packages {
            // Example content data (package, path, type)
            let example_contents = vec![
                (format!("{}@{}", pkg_info.name, pkg_info.version), "/usr/bin/example".to_string(), "file".to_string()),
                (format!("{}@{}", pkg_info.name, pkg_info.version), "/usr/share/doc/example".to_string(), "dir".to_string()),
            ];
            
            // Filter by action type if specified
            let filtered_contents = if let Some(types) = action_types {
                example_contents.into_iter()
                    .filter(|(_, _, action_type)| types.contains(&action_type))
                    .collect::<Vec<_>>()
            } else {
                example_contents
            };
            
            contents.extend(filtered_contents);
        }
        
        Ok(contents)
    }
    
    /// Rebuild repository metadata
    fn rebuild(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()> {
        // This is a placeholder implementation
        // In a real implementation, we would rebuild catalogs and search indexes
        
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
                // In a real implementation, we would rebuild the search index
            }
        }
        
        Ok(())
    }
    
    /// Refresh repository metadata
    fn refresh(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()> {
        // This is a placeholder implementation
        // In a real implementation, we would refresh catalogs and search indexes
        
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
                // In a real implementation, we would refresh the search index
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