//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use super::{RepositoryError, Result};
use flate2::write::GzEncoder;
use flate2::Compression as GzipCompression;
use lz4::EncoderBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha2Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};
use walkdir::WalkDir;

use crate::actions::{File as FileAction, Manifest};
use crate::digest::Digest;
use crate::fmri::Fmri;
use crate::payload::{Payload, PayloadCompressionAlgorithm};

use super::{
    PackageContents, PackageInfo, PublisherInfo, ReadableRepository, RepositoryConfig,
    RepositoryInfo, RepositoryVersion, WritableRepository, REPOSITORY_CONFIG_FILENAME,
};
use super::catalog_writer;
use ini::Ini;

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
        self.terms
            .entry(term)
            .or_insert_with(HashSet::new)
            .insert(fmri.to_string());

        // Add the package to the package map
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
        // Convert a query to lowercase for case-insensitive search
        let query = query.to_lowercase();

        // Split the query into terms
        let terms: Vec<&str> = query.split_whitespace().collect();

        // If no terms, return an empty result
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
                    result_set = result_set.map(|rs| rs.intersection(packages).cloned().collect());
                }
            } else {
                // If any term has no matches, the result is empty
                return Vec::new();
            }
        }

        // Convert the result set to a vector
        let mut results: Vec<String> = result_set.unwrap_or_default().into_iter().collect();

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
    /// Uses RefCell for interior mutability to allow mutation through immutable references
    catalog_manager: Option<std::cell::RefCell<crate::repository::catalog::CatalogManager>>,
    /// Manager for obsoleted packages
    obsoleted_manager: Option<std::cell::RefCell<crate::repository::obsoleted::ObsoletedPackageManager>>,
}

/// Format a SystemTime as an ISO 8601 timestamp string
fn format_iso8601_timestamp(time: &SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));

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

/// Transaction for publishing packages
pub struct Transaction {
    /// Unique ID for the transaction
    #[allow(dead_code)]
    id: String,
    /// Path to the transaction directory
    path: PathBuf,
    /// Manifest being updated
    manifest: Manifest,
    /// Files to be published
    files: Vec<(PathBuf, String)>, // (source_path, sha256)
    /// Repository reference
    repo: PathBuf,
    /// Publisher name
    publisher: Option<String>,
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

        // Create a transaction directory
        let trans_path = repo_path.join("trans").join(&id);
        fs::create_dir_all(&trans_path)?;

        Ok(Transaction {
            id,
            path: trans_path,
            manifest: Manifest::new(),
            files: Vec::new(),
            repo: repo_path,
            publisher: None,
        })
    }

    /// Set the publisher for this transaction
    pub fn set_publisher(&mut self, publisher: &str) {
        self.publisher = Some(publisher.to_string());
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
        let existing_file_paths: HashSet<String> =
            self.manifest.files.iter().map(|f| f.path.clone()).collect();

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
            fs::remove_file(&temp_file_path).map_err(|e| {
                RepositoryError::FileWriteError {
                    path: temp_file_path.clone(),
                    source: e,
                }
            })?;
        }

        // Read the file content
        let file_content = fs::read(file_path).map_err(|e| {
            RepositoryError::FileReadError {
                path: file_path.to_path_buf(),
                source: e,
            }
        })?;

        // Create a payload with the hash information if it doesn't exist
        let mut updated_file_action = file_action;
        let mut payload = updated_file_action.payload.unwrap_or_else(Payload::default);

        // Set the compression algorithm (use the one from payload or default to Gzip)
        let compression_algorithm = payload.compression_algorithm;

        // Compress the file based on the selected algorithm
        let compressed_hash = match compression_algorithm {
            PayloadCompressionAlgorithm::Gzip => {
                // Create a Gzip encoder with the default compression level
                let mut encoder = GzEncoder::new(Vec::new(), GzipCompression::default());

                // Write the file content to the encoder
                encoder.write_all(&file_content).map_err(|e| {
                    RepositoryError::Other(format!("Failed to write data to Gzip encoder: {}", e))
                })?;

                // Finish the compression and get the compressed data
                let compressed_data = encoder.finish().map_err(|e| {
                    RepositoryError::Other(format!("Failed to finish Gzip compression: {}", e))
                })?;

                // Write the compressed data to the temp file
                fs::write(&temp_file_path, &compressed_data).map_err(|e| {
                    RepositoryError::FileWriteError {
                        path: temp_file_path.clone(),
                        source: e,
                    }
                })?;

                // Calculate hash of the compressed data
                let mut hasher = Sha256::new();
                hasher.update(&compressed_data);
                format!("{:x}", hasher.finalize())
            }
            PayloadCompressionAlgorithm::LZ4 => {
                // Create an LZ4 encoder with the default compression level
                let mut encoder = EncoderBuilder::new().build(Vec::new()).map_err(|e| {
                    RepositoryError::Other(format!("Failed to create LZ4 encoder: {}", e))
                })?;

                // Write the file content to the encoder
                encoder.write_all(&file_content).map_err(|e| {
                    RepositoryError::Other(format!("Failed to write data to LZ4 encoder: {}", e))
                })?;

                // Finish the compression and get the compressed data
                let (compressed_data, _) = encoder.finish();

                // Write the compressed data to the temp file
                fs::write(&temp_file_path, &compressed_data).map_err(|e| {
                    RepositoryError::FileWriteError {
                        path: temp_file_path.clone(),
                        source: e,
                    }
                })?;

                // Calculate hash of the compressed data
                let mut hasher = Sha256::new();
                hasher.update(&compressed_data);
                format!("{:x}", hasher.finalize())
            }
        };

        // Add a file to the list for later processing during commit
        self.files
            .push((temp_file_path.clone(), compressed_hash.clone()));

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

        // Determine the publisher to use
        let publisher = match &self.publisher {
            Some(pub_name) => {
                debug!("Using specified publisher: {}", pub_name);
                pub_name.clone()
            }
            None => {
                debug!("No publisher specified, trying to use default publisher");
                // If no publisher is specified, use the default publisher from the repository config
                let config_path = self.repo.join(REPOSITORY_CONFIG_FILENAME);
                if config_path.exists() {
                    let config_content = fs::read_to_string(&config_path)?;
                    let config: RepositoryConfig = serde_json::from_str(&config_content)?;
                    match config.default_publisher {
                        Some(default_pub) => {
                            debug!("Using default publisher: {}", default_pub);
                            default_pub
                        }
                        None => {
                            debug!("No default publisher set in repository");
                            return Err(RepositoryError::Other(
                                "No publisher specified and no default publisher set in repository"
                                    .to_string(),
                            ));
                        }
                    }
                } else {
                    debug!("Repository configuration not found");
                    return Err(RepositoryError::Other(
                        "No publisher specified and repository configuration not found".to_string(),
                    ));
                }
            }
        };

        // Copy files to their final location
        for (source_path, hash) in self.files {
            // Create the destination path using the helper function with publisher
            let dest_path = FileBackend::construct_file_path_with_publisher(&self.repo, &publisher, &hash);

            // Create parent directories if they don't exist
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Copy the file if it doesn't already exist
            if !dest_path.exists() {
                fs::copy(source_path, &dest_path)?;
            }
        }

        // Extract package information from manifest
        let mut package_stem = String::from("unknown");
        let mut package_version = String::from("");
        for attr in &self.manifest.attributes {
            if attr.key == "pkg.fmri" && !attr.values.is_empty() {
                if let Ok(fmri) = Fmri::parse(&attr.values[0]) {
                    package_stem = fmri.stem().to_string();
                    package_version = fmri.version();
                    debug!("Extracted package stem from FMRI: {}", package_stem);
                    debug!("Extracted package version from FMRI: {}", package_version);
                    break;
                }
            }
        }

        // Determine the publisher to use
        let publisher = match &self.publisher {
            Some(pub_name) => {
                debug!("Using specified publisher: {}", pub_name);
                pub_name.clone()
            }
            None => {
                debug!("No publisher specified, trying to use default publisher");
                // If no publisher is specified, use the default publisher from the repository config
                let config_path = self.repo.join(REPOSITORY_CONFIG_FILENAME);
                if config_path.exists() {
                    let config_content = fs::read_to_string(&config_path)?;
                    let config: RepositoryConfig = serde_json::from_str(&config_content)?;
                    match config.default_publisher {
                        Some(default_pub) => {
                            debug!("Using default publisher: {}", default_pub);
                            default_pub
                        }
                        None => {
                            debug!("No default publisher set in repository");
                            return Err(RepositoryError::Other(
                                "No publisher specified and no default publisher set in repository"
                                    .to_string(),
                            ));
                        }
                    }
                } else {
                    debug!("Repository configuration not found");
                    return Err(RepositoryError::Other(
                        "No publisher specified and repository configuration not found".to_string(),
                    ));
                }
            }
        };

        // Create the package directory if it doesn't exist
        let pkg_dir = FileBackend::construct_package_dir(&self.repo, &publisher, &package_stem);
        debug!("Package directory: {}", pkg_dir.display());
        if !pkg_dir.exists() {
            debug!("Creating package directory");
            fs::create_dir_all(&pkg_dir)?;
        }

        // Construct the manifest path using the helper method
        let pkg_manifest_path = if package_version.is_empty() {
            // If no version was provided, store as a default manifest file
            FileBackend::construct_package_dir(&self.repo, &publisher, &package_stem).join("manifest")
        } else {
            FileBackend::construct_manifest_path(
                &self.repo,
                &publisher,
                &package_stem,
                &package_version,
            )
        };
        debug!("Manifest path: {}", pkg_manifest_path.display());

        // Create parent directories if they don't exist
        if let Some(parent) = pkg_manifest_path.parent() {
            debug!("Creating parent directories: {}", parent.display());
            fs::create_dir_all(parent)?;
        }

        // Copy to pkg directory
        debug!(
            "Copying manifest from {} to {}",
            manifest_path.display(),
            pkg_manifest_path.display()
        );
        fs::copy(&manifest_path, &pkg_manifest_path)?;

        // Check if we need to create a pub.p5i file for the publisher
        let config_path = self.repo.join(REPOSITORY_CONFIG_FILENAME);
        if config_path.exists() {
            let config_content = fs::read_to_string(&config_path)?;
            let config: RepositoryConfig = serde_json::from_str(&config_content)?;
            
            // Check if this publisher was just added in this transaction
            let publisher_dir = self.repo.join("publisher").join(&publisher);
            let pub_p5i_path = publisher_dir.join("pub.p5i");
            
            if !pub_p5i_path.exists() {
                debug!("Creating pub.p5i file for publisher: {}", publisher);
                
                // Create the pub.p5i file
                let repo = FileBackend {
                    path: self.repo.clone(),
                    config,
                    catalog_manager: None,
                    obsoleted_manager: None,
                };
                
                repo.create_pub_p5i_file(&publisher)?;
            }
        }

        // Clean up the transaction directory
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

impl ReadableRepository for FileBackend {
    /// Open an existing repository
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Check if the repository directory exists
        if !path.exists() {
            return Err(RepositoryError::NotFound(path.display().to_string()));
        }

        // Load the repository configuration
        // Prefer pkg6.repository (JSON). If absent, try legacy pkg5.repository (INI)
        let config6_path = path.join(REPOSITORY_CONFIG_FILENAME);
        let config5_path = path.join("pkg5.repository");

        let config: RepositoryConfig = if config6_path.exists() {
            let config_data = fs::read_to_string(&config6_path)
                .map_err(|e| RepositoryError::ConfigReadError(format!("{}: {}", config6_path.display(), e)))?;
            serde_json::from_str(&config_data)?
        } else if config5_path.exists() {
            // Minimal mapping for legacy INI: take publishers only from INI; do not scan disk.
            let ini = Ini::load_from_file(&config5_path)
                .map_err(|e| RepositoryError::ConfigReadError(format!("{}: {}", config5_path.display(), e)))?;

            // Default repository version for legacy format is v4
            let mut cfg = RepositoryConfig::default();

            // Try to read default publisher from [publisher] section (key: prefix)
            if let Some(section) = ini.section(Some("publisher")) {
                if let Some(prefix) = section.get("prefix") {
                    cfg.default_publisher = Some(prefix.to_string());
                    cfg.publishers.push(prefix.to_string());
                }
            }

            // If INI enumerates publishers in an optional [publishers] section as comma-separated list
            if let Some(section) = ini.section(Some("publishers")) {
                if let Some(list) = section.get("list") {
                    // replace list strictly by INI contents per requirements
                    cfg.publishers.clear();
                    for p in list.split(',') {
                        let name = p.trim();
                        if !name.is_empty() {
                            cfg.publishers.push(name.to_string());
                        }
                    }
                }
            }

            cfg
        } else {
            return Err(RepositoryError::ConfigReadError(format!(
                "No repository config found: expected {} or {}",
                config6_path.display(),
                config5_path.display()
            )));
        };

        Ok(FileBackend {
            path: path.to_path_buf(),
            config,
            catalog_manager: None,
            obsoleted_manager: None,
        })
    }

    /// Get repository information
    fn get_info(&self) -> Result<RepositoryInfo> {
        let mut publishers = Vec::new();

        for publisher_name in &self.config.publishers {
            // Count packages by scanning the publisher's package directory
            let publisher_pkg_dir = Self::construct_package_dir(&self.path, publisher_name, "");
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
                // If no files were found, use the current time
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

    /// List packages in the repository
    fn list_packages(
        &self,
        publisher: Option<&str>,
        pattern: Option<&str>,
    ) -> Result<Vec<PackageInfo>> {
        let mut packages = Vec::new();

        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(RepositoryError::PublisherNotFound(pub_name.to_string()));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };

        // For each publisher, list packages
        for pub_name in publishers {
            // Get the publisher's package directory
            let publisher_pkg_dir = Self::construct_package_dir(&self.path, &pub_name, "");

            // Check if the publisher directory exists
            if publisher_pkg_dir.exists() {
                // Verify that the publisher is in the config
                if !self.config.publishers.contains(&pub_name) {
                    return Err(RepositoryError::Other(format!(
                        "Publisher directory exists but is not in the repository configuration: {}",
                        pub_name
                    )));
                }

                // Recursively walk through the directory and collect package manifests
                self.find_manifests_recursive(
                    &publisher_pkg_dir,
                    &pub_name,
                    pattern,
                    &mut packages,
                )?;
            }
        }

        Ok(packages)
    }

    /// Show the contents of packages
    fn show_contents(
        &self,
        publisher: Option<&str>,
        pattern: Option<&str>,
        action_types: Option<&[String]>,
    ) -> Result<Vec<PackageContents>> {
        debug!("show_contents called with publisher: {:?}, pattern: {:?}", publisher, pattern);
        // Use a HashMap to store package information
        let mut packages = HashMap::new();

        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(RepositoryError::PublisherNotFound(pub_name.to_string()));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };

        // For each publisher, process packages
        for pub_name in publishers {
            // Get the publisher's package directory
            let publisher_pkg_dir = Self::construct_package_dir(&self.path, &pub_name, "");

            // Check if the publisher directory exists
            if publisher_pkg_dir.exists() {
                // Walk through the directory and collect package manifests
                if let Ok(entries) = fs::read_dir(&publisher_pkg_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();

                        if path.is_dir() {
                            // Recursively search subdirectories
                            if let Ok(subentries) = fs::read_dir(&path) {
                                for subentry in subentries.flatten() {
                                    let subpath = subentry.path();
                                    if subpath.is_file() {
                                        // Try to read the first few bytes of the file to check if it's a manifest file
                                        let mut file = match fs::File::open(&subpath) {
                                            Ok(file) => file,
                                            Err(err) => {
                                                error!(
                                                    "FileBackend::show_contents: Error opening file {}: {}",
                                                    subpath.display(),
                                                    err
                                                );
                                                continue;
                                            }
                                        };

                                        let mut buffer = [0; 1024];
                                        let bytes_read = match file.read(&mut buffer) {
                                            Ok(bytes) => bytes,
                                            Err(err) => {
                                                error!(
                                                    "FileBackend::show_contents: Error reading file {}: {}",
                                                    subpath.display(),
                                                    err
                                                );
                                                continue;
                                            }
                                        };

                                        // Check if the file starts with a valid manifest marker
                                        if bytes_read == 0
                                            || (buffer[0] != b'{' && buffer[0] != b'<' && buffer[0] != b's')
                                        {
                                            continue;
                                        }

                                        // Parse the manifest file to get package information
                                        match Manifest::parse_file(&subpath) {
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
                                                                        }
                                                                        Err(err) => {
                                                                            // Log the error but fall back to the simple string contains
                                                                            error!("FileBackend::show_contents: Error compiling regex pattern '{}': {}", pat, err);
                                                                            if !parsed_fmri.stem().contains(pat) {
                                                                                continue;
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                // Format the package identifier using the FMRI
                                                                let version = parsed_fmri.version();
                                                                pkg_id = if !version.is_empty() {
                                                                    format!(
                                                                        "{}@{}",
                                                                        parsed_fmri.stem(),
                                                                        version
                                                                    )
                                                                } else {
                                                                    parsed_fmri.stem().to_string()
                                                                };

                                                                break;
                                                            }
                                                            Err(err) => {
                                                                // Log the error but continue processing
                                                                error!(
                                                                    "FileBackend::show_contents: Error parsing FMRI '{}': {}",
                                                                    fmri, err
                                                                );
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
                                                if action_types.is_none()
                                                    || action_types
                                                        .as_ref()
                                                        .unwrap()
                                                        .contains(&"file".to_string())
                                                {
                                                    for file in &manifest.files {
                                                        content_vectors.files.push(file.path.clone());
                                                    }
                                                }

                                                // Process directory actions
                                                if action_types.is_none()
                                                    || action_types
                                                        .as_ref()
                                                        .unwrap()
                                                        .contains(&"dir".to_string())
                                                {
                                                    for dir in &manifest.directories {
                                                        content_vectors.directories.push(dir.path.clone());
                                                    }
                                                }

                                                // Process link actions
                                                if action_types.is_none()
                                                    || action_types
                                                        .as_ref()
                                                        .unwrap()
                                                        .contains(&"link".to_string())
                                                {
                                                    for link in &manifest.links {
                                                        content_vectors.links.push(link.path.clone());
                                                    }
                                                }

                                                // Process dependency actions
                                                if action_types.is_none()
                                                    || action_types
                                                        .as_ref()
                                                        .unwrap()
                                                        .contains(&"depend".to_string())
                                                {
                                                    for depend in &manifest.dependencies {
                                                        if let Some(fmri) = &depend.fmri {
                                                            content_vectors.dependencies.push(fmri.to_string());
                                                        }
                                                    }
                                                }

                                                // Process license actions
                                                if action_types.is_none()
                                                    || action_types
                                                        .as_ref()
                                                        .unwrap()
                                                        .contains(&"license".to_string())
                                                {
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
                                            }
                                            Err(err) => {
                                                // Log the error but continue processing other files
                                                error!(
                                                    "FileBackend::show_contents: Error parsing manifest file {}: {}",
                                                    subpath.display(),
                                                    err
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        } else if path.is_file() {
                            // Try to read the first few bytes of the file to check if it's a manifest file
                            let mut file = match fs::File::open(&path) {
                                Ok(file) => file,
                                Err(err) => {
                                    error!(
                                        "FileBackend::show_contents: Error opening file {}: {}",
                                        path.display(),
                                        err
                                    );
                                    continue;
                                }
                            };

                            let mut buffer = [0; 1024];
                            let bytes_read = match file.read(&mut buffer) {
                                Ok(bytes) => bytes,
                                Err(err) => {
                                    error!(
                                        "FileBackend::show_contents: Error reading file {}: {}",
                                        path.display(),
                                        err
                                    );
                                    continue;
                                }
                            };

                            // Check if the file starts with a valid manifest marker
                            // For example, if it's a JSON file, it should start with '{'
                            if bytes_read == 0
                                || (buffer[0] != b'{' && buffer[0] != b'<' && buffer[0] != b's')
                            {
                                continue;
                            }
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
                                                                if !regex
                                                                    .is_match(parsed_fmri.stem())
                                                                {
                                                                    continue;
                                                                }
                                                            }
                                                            Err(err) => {
                                                                // Log the error but fall back to the simple string contains
                                                                error!("FileBackend::show_contents: Error compiling regex pattern '{}': {}", pat, err);
                                                                if !parsed_fmri.stem().contains(pat)
                                                                {
                                                                    continue;
                                                                }
                                                            }
                                                        }
                                                    }

                                                    // Format the package identifier using the FMRI
                                                    let version = parsed_fmri.version();
                                                    pkg_id = if !version.is_empty() {
                                                        format!(
                                                            "{}@{}",
                                                            parsed_fmri.stem(),
                                                            version
                                                        )
                                                    } else {
                                                        parsed_fmri.stem().to_string()
                                                    };

                                                    break;
                                                }
                                                Err(err) => {
                                                    // Log the error but continue processing
                                                    error!(
                                                        "FileBackend::show_contents: Error parsing FMRI '{}': {}",
                                                        fmri, err
                                                    );
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
                                    if action_types.is_none()
                                        || action_types
                                            .as_ref()
                                            .unwrap()
                                            .contains(&"file".to_string())
                                    {
                                        for file in &manifest.files {
                                            content_vectors.files.push(file.path.clone());
                                        }
                                    }

                                    // Process directory actions
                                    if action_types.is_none()
                                        || action_types
                                            .as_ref()
                                            .unwrap()
                                            .contains(&"dir".to_string())
                                    {
                                        for dir in &manifest.directories {
                                            content_vectors.directories.push(dir.path.clone());
                                        }
                                    }

                                    // Process link actions
                                    if action_types.is_none()
                                        || action_types
                                            .as_ref()
                                            .unwrap()
                                            .contains(&"link".to_string())
                                    {
                                        for link in &manifest.links {
                                            content_vectors.links.push(link.path.clone());
                                        }
                                    }

                                    // Process dependency actions
                                    if action_types.is_none()
                                        || action_types
                                            .as_ref()
                                            .unwrap()
                                            .contains(&"depend".to_string())
                                    {
                                        for depend in &manifest.dependencies {
                                            if let Some(fmri) = &depend.fmri {
                                                content_vectors.dependencies.push(fmri.to_string());
                                            }
                                        }
                                    }

                                    // Process license actions
                                    if action_types.is_none()
                                        || action_types
                                            .as_ref()
                                            .unwrap()
                                            .contains(&"license".to_string())
                                    {
                                        for license in &manifest.licenses {
                                            if let Some(path_prop) = license.properties.get("path")
                                            {
                                                content_vectors
                                                    .licenses
                                                    .push(path_prop.value.clone());
                                            } else if let Some(license_prop) =
                                                license.properties.get("license")
                                            {
                                                content_vectors
                                                    .licenses
                                                    .push(license_prop.value.clone());
                                            } else {
                                                content_vectors
                                                    .licenses
                                                    .push(license.payload.clone());
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    // Log the error but continue processing other files
                                    error!(
                                        "FileBackend::show_contents: Error parsing manifest file {}: {}",
                                        path.display(),
                                        err
                                    );
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

    fn fetch_payload(&mut self, publisher: &str, digest: &str, dest: &Path) -> Result<()> {
        // Parse digest; supports both raw hash and source:algorithm:hash
        let parsed = match Digest::from_str(digest) {
            Ok(d) => d,
            Err(e) => return Err(RepositoryError::DigestError(e.to_string())),
        };
        let hash = parsed.hash.clone();
        let algo = parsed.algorithm.clone();

        if hash.is_empty() {
            return Err(RepositoryError::Other("Empty digest provided".to_string()));
        }

        // Prepare candidate paths (prefer publisher-specific, then global)
        let cand_pub = Self::construct_file_path_with_publisher(&self.path, publisher, &hash);
        let cand_global = Self::construct_file_path(&self.path, &hash);

        let source_path = if cand_pub.exists() {
            cand_pub
        } else if cand_global.exists() {
            cand_global
        } else {
            return Err(RepositoryError::NotFound(format!(
                "payload {} not found in repository",
                hash
            )));
        };

        // Ensure destination directory exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        // If destination already exists and matches digest, do nothing
        if dest.exists() {
            let bytes = fs::read(dest).map_err(|e| RepositoryError::FileReadError { path: dest.to_path_buf(), source: e })?;
            match crate::digest::Digest::from_bytes(&bytes, algo.clone(), crate::digest::DigestSource::PrimaryPayloadHash) {
                Ok(comp) if comp.hash == hash => return Ok(()),
                _ => { /* fall through to overwrite */ }
            }
        }

        // Read source content and verify digest
        let bytes = fs::read(&source_path).map_err(|e| RepositoryError::FileReadError { path: source_path.clone(), source: e })?;
        match crate::digest::Digest::from_bytes(&bytes, algo, crate::digest::DigestSource::PrimaryPayloadHash) {
            Ok(comp) => {
                if comp.hash != hash {
                    return Err(RepositoryError::DigestError(format!(
                        "Digest mismatch: expected {}, got {}",
                        hash, comp.hash
                    )));
                }
            }
            Err(e) => return Err(RepositoryError::DigestError(e.to_string())),
        }

        // Write atomically
        let tmp = dest.with_extension("tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&bytes)?;
        }
        fs::rename(&tmp, dest)?;

        Ok(())
    }

    fn fetch_manifest(
        &mut self,
        publisher: &str,
        fmri: &crate::fmri::Fmri,
    ) -> Result<crate::actions::Manifest> {
        // Require a concrete version
        let version = fmri.version();
        if version.is_empty() {
            return Err(RepositoryError::Other("FMRI must include a version to fetch manifest".into()));
        }

        // Preferred path: publisher-scoped manifest path
        let path = Self::construct_manifest_path(&self.path, publisher, fmri.stem(), &version);
        if path.exists() {
            return crate::actions::Manifest::parse_file(&path).map_err(RepositoryError::from);
        }

        // Fallbacks: global pkg layout without publisher
        let encoded_stem = Self::url_encode(fmri.stem());
        let encoded_version = Self::url_encode(&version);
        let alt1 = self.path.join("pkg").join(&encoded_stem).join(&encoded_version);
        if alt1.exists() {
            return crate::actions::Manifest::parse_file(&alt1).map_err(RepositoryError::from);
        }

        let alt2 = self
            .path
            .join("publisher")
            .join(publisher)
            .join("pkg")
            .join(&encoded_stem)
            .join(&encoded_version);
        if alt2.exists() {
            return crate::actions::Manifest::parse_file(&alt2).map_err(RepositoryError::from);
        }

        Err(RepositoryError::NotFound(format!(
            "manifest for {} not found",
            fmri
        )))
    }

    /// Search for packages in the repository
    fn search(
        &self,
        query: &str,
        publisher: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<PackageInfo>> {
        debug!("Searching for packages with query: {}", query);
        debug!("Publisher: {:?}", publisher);
        debug!("Limit: {:?}", limit);

        // If no publisher is specified, use the default publisher if available
        let publisher = publisher.or_else(|| self.config.default_publisher.as_deref());
        debug!("Effective publisher: {:?}", publisher);

        // If still no publisher, we need to search all publishers
        let publishers = if let Some(pub_name) = publisher {
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };
        debug!("Publishers to search: {:?}", publishers);

        let mut results = Vec::new();

        // For each publisher, search the index
        for pub_name in publishers {
            debug!("Searching publisher: {}", pub_name);

            // Check if the index exists
            let index_path = self.path.join("index").join(&pub_name).join("search.json");
            debug!("Index path: {}", index_path.display());
            debug!("Index exists: {}", index_path.exists());

            if let Ok(Some(index)) = self.get_search_index(&pub_name) {
                debug!("Got search index for publisher: {}", pub_name);
                debug!("Index terms: {:?}", index.terms.keys().collect::<Vec<_>>());

                // Search the index
                let fmris = index.search(query, limit);
                debug!("Search results (FMRIs): {:?}", fmris);

                // Convert FMRIs to PackageInfo
                for fmri_str in fmris {
                    if let Ok(fmri) = Fmri::parse(&fmri_str) {
                        debug!("Adding package to results: {}", fmri);
                        results.push(PackageInfo { fmri });
                    } else {
                        debug!("Failed to parse FMRI: {}", fmri_str);
                    }
                }
            } else {
                debug!("No search index found for publisher: {}", pub_name);
                debug!("Falling back to simple search");

                // If the index doesn't exist, fall back to the simple search
                let all_packages = self.list_packages(Some(&pub_name), None)?;
                debug!("All packages: {:?}", all_packages);

                // Filter packages by the query string
                let matching_packages: Vec<PackageInfo> = all_packages
                    .into_iter()
                    .filter(|pkg| {
                        // Match against package name
                        let matches = pkg.fmri.stem().contains(query);
                        debug!("Package: {}, Matches: {}", pkg.fmri.stem(), matches);
                        matches
                    })
                    .collect();
                debug!("Matching packages: {:?}", matching_packages);

                // Add matching packages to the results
                results.extend(matching_packages);
            }
        }

        // Apply limit if specified
        if let Some(max_results) = limit {
            results.truncate(max_results);
        }

        debug!("Final search results: {:?}", results);
        Ok(results)
    }
}

impl WritableRepository for FileBackend {
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
            obsoleted_manager: None,
        };

        // Create the repository directories
        repo.create_directories()?;

        // Save the repository configuration
        repo.save_config()?;

        Ok(repo)
    }

    /// Save the repository configuration
    fn save_config(&self) -> Result<()> {
        // Save the modern JSON format
        let config_path = self.path.join(REPOSITORY_CONFIG_FILENAME);
        let config_data = serde_json::to_string_pretty(&self.config)?;
        fs::write(config_path, config_data)?;
        
        // Save the legacy INI format for backward compatibility
        self.save_legacy_config()?;
        
        Ok(())
    }

    /// Add a publisher to the repository
    fn add_publisher(&mut self, publisher: &str) -> Result<()> {
        if !self.config.publishers.contains(&publisher.to_string()) {
            self.config.publishers.push(publisher.to_string());

            // Create publisher-specific directories
            fs::create_dir_all(Self::construct_catalog_path(&self.path, publisher))?;
            fs::create_dir_all(Self::construct_package_dir(&self.path, publisher, ""))?;

            // Create the publisher directory if it doesn't exist
            let publisher_dir = self.path.join("publisher").join(publisher);
            fs::create_dir_all(&publisher_dir)?;

            // Create the pub.p5i file for backward compatibility
            self.create_pub_p5i_file(publisher)?;

            // Set as the default publisher if no default publisher is set
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
                let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
                let pkg_dir = Self::construct_package_dir(&self.path, publisher, "");

                // Remove the catalog directory if it exists
                if catalog_dir.exists() {
                    fs::remove_dir_all(&catalog_dir).map_err(|e| {
                        RepositoryError::Other(format!("Failed to remove catalog directory: {}", e))
                    })?;
                }

                // Remove the package directory if it exists
                if pkg_dir.exists() {
                    fs::remove_dir_all(&pkg_dir).map_err(|e| {
                        RepositoryError::Other(format!("Failed to remove package directory: {}", e))
                    })?;
                }

                // Save the updated configuration
                self.save_config()?;
            }
        }

        Ok(())
    }

    /// Set a repository property
    fn set_property(&mut self, property: &str, value: &str) -> Result<()> {
        self.config
            .properties
            .insert(property.to_string(), value.to_string());
        self.save_config()?;
        Ok(())
    }

    /// Set a publisher property
    fn set_publisher_property(
        &mut self,
        publisher: &str,
        property: &str,
        value: &str,
    ) -> Result<()> {
        // Check if the publisher exists
        if !self.config.publishers.contains(&publisher.to_string()) {
            return Err(RepositoryError::PublisherNotFound(publisher.to_string()));
        }

        // Create the property key in the format "publisher/property"
        let key = format!("{}/{}", publisher, property);

        // Set the property
        self.config.properties.insert(key, value.to_string());

        // Save the updated configuration
        self.save_config()?;

        Ok(())
    }

    /// Rebuild repository metadata
    fn rebuild(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()> {
        debug!(
            "rebuild called with publisher: {:?}, no_catalog: {}, no_index: {}",
            publisher, no_catalog, no_index
        );

        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(RepositoryError::PublisherNotFound(pub_name.to_string()));
            }
            debug!("rebuild: using specified publisher: {}", pub_name);
            vec![pub_name.to_string()]
        } else {
            debug!(
                "rebuild: using all publishers: {:?}",
                self.config.publishers
            );
            self.config.publishers.clone()
        };

        // For each publisher, rebuild metadata
        for pub_name in publishers {
            info!("Rebuilding metadata for publisher: {}", pub_name);

            if !no_catalog {
                info!("Rebuilding catalog...");
                self.rebuild_catalog(&pub_name, true)?;
            }

            if !no_index {
                info!("Rebuilding search index...");
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
                return Err(RepositoryError::PublisherNotFound(pub_name.to_string()));
            }
            vec![pub_name.to_string()]
        } else {
            self.config.publishers.clone()
        };

        // For each publisher, refresh metadata
        for pub_name in publishers {
            info!("Refreshing metadata for publisher: {}", pub_name);

            if !no_catalog {
                info!("Refreshing catalog...");
                self.rebuild_catalog(&pub_name, true)?;
            }

            if !no_index {
                info!("Refreshing search index...");
                self.build_search_index(&pub_name)?;
            }
        }

        Ok(())
    }

    /// Set the default publisher for the repository
    fn set_default_publisher(&mut self, publisher: &str) -> Result<()> {
        // Check if the publisher exists
        if !self.config.publishers.contains(&publisher.to_string()) {
            return Err(RepositoryError::PublisherNotFound(publisher.to_string()));
        }

        // Set the default publisher
        self.config.default_publisher = Some(publisher.to_string());

        // Save the updated configuration
        self.save_config()?;

        Ok(())
    }
}

impl FileBackend {
    /// Save catalog.attrs for a publisher using atomic write and SHA-1 signature
    pub fn save_catalog_attrs(
        &self,
        publisher: &str,
        attrs: &mut crate::repository::catalog::CatalogAttrs,
    ) -> Result<String> {
        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        std::fs::create_dir_all(&catalog_dir)?;
        let attrs_path = catalog_dir.join("catalog.attrs");
        super::catalog_writer::write_catalog_attrs(&attrs_path, attrs)
    }

    /// Save a catalog part for a publisher using atomic write and SHA-1 signature
    pub fn save_catalog_part(
        &self,
        publisher: &str,
        part_name: &str,
        part: &mut crate::repository::catalog::CatalogPart,
    ) -> Result<String> {
        if part_name.contains('/') || part_name.contains('\\') {
            return Err(RepositoryError::PathPrefixError(part_name.to_string()));
        }
        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        std::fs::create_dir_all(&catalog_dir)?;
        let part_path = catalog_dir.join(part_name);
        super::catalog_writer::write_catalog_part(&part_path, part)
    }

    /// Append a single update entry to the current update log file for a publisher and locale.
    /// If no current log exists, creates one using current timestamp.
    pub fn append_update(
        &self,
        publisher: &str,
        locale: &str,
        fmri: &crate::fmri::Fmri,
        op_type: crate::repository::catalog::CatalogOperationType,
        catalog_parts: std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>,
        signature_sha1: Option<String>,
    ) -> Result<()> {
        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        std::fs::create_dir_all(&catalog_dir)?;

        // Locate latest update file for locale
        let mut latest: Option<PathBuf> = None;
        if let Ok(read_dir) = std::fs::read_dir(&catalog_dir) {
            for e in read_dir.flatten() {
                let p = e.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("update.") && name.ends_with(&format!(".{}", locale)) {
                        if latest.as_ref().map(|lp| p > *lp).unwrap_or(true) {
                            latest = Some(p);
                        }
                    }
                }
            }
        }

        // If none, create a new filename using current timestamp in basic format
        let update_path = match latest {
            Some(p) => p,
            None => {
                let now = std::time::SystemTime::now();
                let ts = format_iso8601_timestamp(&now); // e.g., 20090508T161025.686485Z
                let stem = ts.split('.').next().unwrap_or(&ts); // take up to seconds
                catalog_dir.join(format!("update.{}.{}", stem, locale))
            }
        };

        // Load or create log
        let mut log = if update_path.exists() {
            crate::repository::catalog::UpdateLog::load(&update_path)?
        } else {
            crate::repository::catalog::UpdateLog::new()
        };

        // Append entry
        log.add_update(publisher, fmri, op_type, catalog_parts, signature_sha1);
        let _ = super::catalog_writer::write_update_log(&update_path, &mut log)?;
        Ok(())
    }

    /// Rotate the update log file by creating a new empty file with the provided timestamp (basic format).
    /// If `timestamp_basic` is None, the current time is used. Timestamp should match catalog v1 naming: YYYYMMDDThhmmssZ
    pub fn rotate_update_file(
        &self,
        publisher: &str,
        locale: &str,
        timestamp_basic: Option<String>,
    ) -> Result<PathBuf> {
        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        std::fs::create_dir_all(&catalog_dir)?;
        let ts_basic = match timestamp_basic {
            Some(s) => s,
            None => {
                let now = std::time::SystemTime::now();
                let ts = format_iso8601_timestamp(&now);
                ts.split('.').next().unwrap_or(&ts).to_string()
            }
        };
        let path = catalog_dir.join(format!("update.{}.{}", ts_basic, locale));
        let mut log = crate::repository::catalog::UpdateLog::new();
        let _ = super::catalog_writer::write_update_log(&path, &mut log)?;
        Ok(path)
    }
    pub fn fetch_manifest_text(&self, publisher: &str, fmri: &Fmri) -> Result<String> {
        // Require a concrete version
        let version = fmri.version();
        if version.is_empty() {
            return Err(RepositoryError::Other("FMRI must include a version to fetch manifest".into()));
        }
        // Preferred path: publisher-scoped manifest path
        let path = Self::construct_manifest_path(&self.path, publisher, fmri.stem(), &version);
        if path.exists() {
            return std::fs::read_to_string(&path).map_err(|e| RepositoryError::FileReadError { path, source: e });
        }
        // Fallbacks: global pkg layout without publisher
        let encoded_stem = Self::url_encode(fmri.stem());
        let encoded_version = Self::url_encode(&version);
        let alt1 = self.path.join("pkg").join(&encoded_stem).join(&encoded_version);
        if alt1.exists() {
            return std::fs::read_to_string(&alt1).map_err(|e| RepositoryError::FileReadError { path: alt1, source: e });
        }
        let alt2 = self
            .path
            .join("publisher")
            .join(publisher)
            .join("pkg")
            .join(&encoded_stem)
            .join(&encoded_version);
        if alt2.exists() {
            return std::fs::read_to_string(&alt2).map_err(|e| RepositoryError::FileReadError { path: alt2, source: e });
        }
        Err(RepositoryError::NotFound(format!("manifest for {} not found", fmri)))
    }
    /// Fetch catalog file path
    pub fn get_catalog_file_path(&self, publisher: &str, filename: &str) -> Result<PathBuf> {
        if filename.contains('/') || filename.contains('\\') {
            return Err(RepositoryError::PathPrefixError(filename.to_string()));
        }

        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        let path = catalog_dir.join(filename);

        if path.exists() {
            Ok(path)
        } else {
            Err(RepositoryError::NotFound(format!(
                "Catalog file {} for publisher {} not found",
                filename, publisher
            )))
        }
    }

    /// Save the legacy pkg5.repository INI file for backward compatibility
    pub fn save_legacy_config(&self) -> Result<()> {
        let legacy_config_path = self.path.join("pkg5.repository");
        let mut conf = Ini::new();
        
        // Add publisher section with default publisher
        if let Some(default_publisher) = &self.config.default_publisher {
            conf.with_section(Some("publisher"))
                .set("prefix", default_publisher);
        }
        
        // Add repository section with version and default values
        conf.with_section(Some("repository"))
            .set("version", "4")
            .set("trust-anchor-directory", "/etc/certs/CA/")
            .set("signature-required-names", "[]")
            .set("check-certificate-revocation", "False");
        
        // Add CONFIGURATION section with version
        conf.with_section(Some("CONFIGURATION"))
            .set("version", "4");
        
        // Write the INI file
        conf.write_to_file(legacy_config_path)?;
        
        Ok(())
    }

    /// Create a pub.p5i file for a publisher for backward compatibility
    /// 
    /// Format: base_path/publisher/publisher_name/pub.p5i
    fn create_pub_p5i_file(&self, publisher: &str) -> Result<()> {
        // Define the structure for the pub.p5i file
        #[derive(serde::Serialize)]
        struct P5iPublisherInfo {
            alias: Option<String>,
            name: String,
            packages: Vec<String>,
            repositories: Vec<String>,
        }

        #[derive(serde::Serialize)]
        struct P5iFile {
            packages: Vec<String>,
            publishers: Vec<P5iPublisherInfo>,
            version: u32,
        }

        // Create the publisher info
        let publisher_info = P5iPublisherInfo {
            alias: None,
            name: publisher.to_string(),
            packages: Vec::new(),
            repositories: Vec::new(),
        };

        // Create the p5i file content
        let p5i_content = P5iFile {
            packages: Vec::new(),
            publishers: vec![publisher_info],
            version: 1,
        };

        // Serialize to JSON
        let json_content = serde_json::to_string_pretty(&p5i_content)?;

        // Create the path for the pub.p5i file
        let pub_p5i_path = self.path.join("publisher").join(publisher).join("pub.p5i");

        // Write the file
        fs::write(pub_p5i_path, json_content)?;

        Ok(())
    }

    /// Helper method to construct a catalog path consistently
    /// 
    /// Format: base_path/publisher/publisher_name/catalog
    pub fn construct_catalog_path(
        base_path: &Path,
        publisher: &str,
    ) -> PathBuf {
        base_path.join("publisher").join(publisher).join("catalog")
    }

    /// Helper method to construct a manifest path consistently
    /// 
    /// Format: base_path/publisher/publisher_name/pkg/stem/encoded_version
    pub fn construct_manifest_path(
        base_path: &Path,
        publisher: &str,
        stem: &str,
        version: &str,
    ) -> PathBuf {
        let pkg_dir = Self::construct_package_dir(base_path, publisher, stem);
        let encoded_version = Self::url_encode(version);
        pkg_dir.join(encoded_version)
    }
    
    /// Helper method to construct a package directory path consistently
    /// 
    /// Format: base_path/publisher/publisher_name/pkg/url_encoded_stem
    pub fn construct_package_dir(
        base_path: &Path,
        publisher: &str,
        stem: &str,
    ) -> PathBuf {
        let encoded_stem = Self::url_encode(stem);
        base_path.join("publisher").join(publisher).join("pkg").join(encoded_stem)
    }
    
    /// Helper method to construct a file path consistently
    /// 
    /// Format: base_path/file/XX/hash
    /// Where XX is the first two characters of the hash
    pub fn construct_file_path(
        base_path: &Path,
        hash: &str,
    ) -> PathBuf {
        if hash.len() < 2 {
            // Fallback for very short hashes (shouldn't happen with SHA256)
            base_path.join("file").join(hash)
        } else {
            // Extract the first two characters from the hash
            let first_two = &hash[0..2];

            // Create the path: $REPO/file/XX/XXYY...
            base_path
                .join("file")
                .join(first_two)
                .join(hash)
        }
    }
    
    /// Helper method to construct a file path consistently with publisher
    /// 
    /// Format: base_path/publisher/publisher_name/file/XX/hash
    /// Where XX is the first two characters of the hash
    pub fn construct_file_path_with_publisher(
        base_path: &Path,
        publisher: &str,
        hash: &str,
    ) -> PathBuf {
        if hash.len() < 2 {
            // Fallback for very short hashes (shouldn't happen with SHA256)
            base_path.join("publisher").join(publisher).join("file").join(hash)
        } else {
            // Extract the first two characters from the hash
            let first_two = &hash[0..2];

            // Create the path: $REPO/publisher/publisher_name/file/XX/XXYY...
            base_path
                .join("publisher")
                .join(publisher)
                .join("file")
                .join(first_two)
                .join(hash)
        }
    }

    /// Recursively find manifest files in a directory and its subdirectories
    fn find_manifests_recursive(
        &self,
        dir: &Path,
        publisher: &str,
        pattern: Option<&str>,
        packages: &mut Vec<PackageInfo>,
    ) -> Result<()> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    // Recursively search subdirectories
                    self.find_manifests_recursive(&path, publisher, pattern, packages)?;
                } else if path.is_file() {
                    // Try to read the first few bytes of the file to check if it's a manifest file
                    let mut file = match fs::File::open(&path) {
                        Ok(file) => file,
                        Err(err) => {
                            error!(
                                "FileBackend::find_manifests_recursive: Error opening file {}: {}",
                                path.display(),
                                err
                            );
                            continue;
                        }
                    };

                    let mut buffer = [0; 1024];
                    let bytes_read = match file.read(&mut buffer) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            error!(
                                "FileBackend::find_manifests_recursive: Error reading file {}: {}",
                                path.display(),
                                err
                            );
                            continue;
                        }
                    };

                    // Check if the file starts with a valid manifest marker
                    // For example, if it's a JSON file, it should start with '{'
                    if bytes_read == 0
                        || (buffer[0] != b'{' && buffer[0] != b'<' && buffer[0] != b's')
                    {
                        continue;
                    }

                    // Process manifest files
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
                                                    }
                                                    Err(err) => {
                                                        // Log the error but fall back to the simple string contains
                                                        error!("FileBackend::find_manifests_recursive: Error compiling regex pattern '{}': {}", pat, err);
                                                        if !parsed_fmri.stem().contains(pat) {
                                                            continue;
                                                        }
                                                    }
                                                }
                                            }

                                            // If the publisher is not set in the FMRI, use the current publisher
                                            let final_fmri = if parsed_fmri.publisher.is_none() {
                                                let mut fmri_with_publisher = parsed_fmri.clone();
                                                fmri_with_publisher.publisher =
                                                    Some(publisher.to_string());
                                                fmri_with_publisher
                                            } else {
                                                parsed_fmri.clone()
                                            };
                                            
                                            // Check if the package is obsoleted
                                            let is_obsoleted = if let Some(obsoleted_manager) = &self.obsoleted_manager {
                                                obsoleted_manager.borrow().is_obsoleted(publisher, &final_fmri)
                                            } else {
                                                false
                                            };
                                            
                                            // Only add the package if it's not obsoleted
                                            if !is_obsoleted {
                                                // Create a PackageInfo struct and add it to the list
                                                packages.push(PackageInfo {
                                                    fmri: final_fmri,
                                                });
                                            }

                                            // Found the package info, no need to check other attributes
                                            break;
                                        }
                                        Err(err) => {
                                            // Log the error but continue processing
                                            error!(
                                                "FileBackend::find_manifests_recursive: Error parsing FMRI '{}': {}",
                                                fmri, err
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            // Log the error but continue processing other files
                            error!(
                                "FileBackend::find_manifests_recursive: Error parsing manifest file {}: {}",
                                path.display(),
                                err
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }
    /// Create the repository directories
    fn create_directories(&self) -> Result<()> {
        // Create the main repository directories
        fs::create_dir_all(self.path.join("publisher"))?;
        fs::create_dir_all(self.path.join("index"))?;
        fs::create_dir_all(self.path.join("trans"))?;
        fs::create_dir_all(self.path.join("obsoleted"))?;

        Ok(())
    }

    /// Rebuild catalog for a publisher
    ///
    /// This method generates catalog files for a publisher and stores them in the publisher's
    /// catalog directory.
    pub fn rebuild_catalog(&self, publisher: &str, create_update_log: bool) -> Result<()> {
        info!("Rebuilding catalog for publisher: {}", publisher);
        
        // Create the catalog directory for the publisher if it doesn't exist
        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        debug!("Publisher catalog directory: {}", catalog_dir.display());
        fs::create_dir_all(&catalog_dir)?;
        debug!("Created publisher catalog directory");

        // Collect package data
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

            // Get the package version
            let version = fmri.version();

            // Construct the manifest path using the helper method
            let manifest_path =
                Self::construct_manifest_path(&self.path, publisher, stem, &version);

            // Check if the package directory exists
            if let Some(pkg_dir) = manifest_path.parent() {
                if !pkg_dir.exists() {
                    error!(
                        "Package directory {} does not exist skipping",
                        pkg_dir.display()
                    );
                    continue;
                }
            }

            if !manifest_path.exists() {
                continue;
            }

            // Read the manifest content for hash calculation
            let manifest_content = fs::read_to_string(&manifest_path).map_err(|e| RepositoryError::FileReadError { path: manifest_path.clone(), source: e })?;

            // Parse the manifest using parse_file which handles JSON correctly
            let manifest = Manifest::parse_file(&manifest_path)?;

            // Calculate SHA-1 hash of the manifest for legacy catalog signature compatibility
            let mut hasher = sha1::Sha1::new();
            hasher.update(manifest_content.as_bytes());
            let signature = hasher.finalize();
            let signature = format!("{:x}", signature);

            // Add to base entries
            base_entries.push((fmri.clone(), None, signature.clone()));

            // Extract dependency actions
            let mut dependency_actions = Vec::new();
            for dep in &manifest.dependencies {
                if let Some(dep_fmri) = &dep.fmri {
                    dependency_actions.push(format!(
                        "depend fmri={} type={}",
                        dep_fmri, dep.dependency_type
                    ));
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
                dependency_entries.push((
                    fmri.clone(),
                    Some(dependency_actions.clone()),
                    signature.clone(),
                ));
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
                summary_entries.push((
                    fmri.clone(),
                    Some(summary_actions.clone()),
                    signature.clone(),
                ));
            }

            // Prepare update entry if needed
            if create_update_log {
                let mut catalog_parts = HashMap::new();

                // Add dependency actions to update entry
                if !dependency_actions.is_empty() {
                    let mut actions = HashMap::new();
                    actions.insert("actions".to_string(), dependency_actions);
                    catalog_parts.insert("catalog.dependency.C".to_string(), actions);
                }

                // Add summary actions to update entry
                if !summary_actions.is_empty() {
                    let mut actions = HashMap::new();
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

        // Create and save catalog parts

        // Create a catalog.attrs file
        let now = SystemTime::now();
        let timestamp = format_iso8601_timestamp(&now);

        // Get the CatalogAttrs struct definition to see what fields it has
        let mut attrs = crate::repository::catalog::CatalogAttrs {
            created: timestamp.clone(),
            last_modified: timestamp.clone(),
            package_count,
            package_version_count,
            parts: HashMap::new(),
            version: 1, // CatalogVersion::V1 is 1
            signature: None,
            updates: HashMap::new(),
        };

        // Add part information
        let base_part_name = "catalog.base.C";
        attrs.parts.insert(
            base_part_name.to_string(),
            crate::repository::catalog::CatalogPartInfo {
                last_modified: timestamp.clone(),
                signature_sha1: None,
            },
        );

        let dependency_part_name = "catalog.dependency.C";
        attrs.parts.insert(
            dependency_part_name.to_string(),
            crate::repository::catalog::CatalogPartInfo {
                last_modified: timestamp.clone(),
                signature_sha1: None,
            },
        );

        let summary_part_name = "catalog.summary.C";
        attrs.parts.insert(
            summary_part_name.to_string(),
            crate::repository::catalog::CatalogPartInfo {
                last_modified: timestamp.clone(),
                signature_sha1: None,
            },
        );

        // Create and save catalog parts

        // Base part
        let base_part_path = catalog_dir.join(base_part_name);
        debug!("Writing base part to: {}", base_part_path.display());
        let mut base_part = crate::repository::catalog::CatalogPart::new();
        for (fmri, actions, signature) in base_entries {
            base_part.add_package(publisher, &fmri, actions, Some(signature));
        }
        let base_sig = catalog_writer::write_catalog_part(&base_part_path, &mut base_part)?;
        debug!("Wrote base part file");

        // Dependency part
        let dependency_part_path = catalog_dir.join(dependency_part_name);
        debug!(
            "Writing dependency part to: {}",
            dependency_part_path.display()
        );
        let mut dependency_part = crate::repository::catalog::CatalogPart::new();
        for (fmri, actions, signature) in dependency_entries {
            dependency_part.add_package(publisher, &fmri, actions, Some(signature));
        }
        let dependency_sig = catalog_writer::write_catalog_part(&dependency_part_path, &mut dependency_part)?;
        debug!("Wrote dependency part file");

        // Summary part
        let summary_part_path = catalog_dir.join(summary_part_name);
        debug!("Writing summary part to: {}", summary_part_path.display());
        let mut summary_part = crate::repository::catalog::CatalogPart::new();
        for (fmri, actions, signature) in summary_entries {
            summary_part.add_package(publisher, &fmri, actions, Some(signature));
        }
        let summary_sig = catalog_writer::write_catalog_part(&summary_part_path, &mut summary_part)?;
        debug!("Wrote summary part file");

        // Update part signatures in attrs (written after parts)
        if let Some(info) = attrs.parts.get_mut(base_part_name) {
            info.signature_sha1 = Some(base_sig);
        }
        if let Some(info) = attrs.parts.get_mut(dependency_part_name) {
            info.signature_sha1 = Some(dependency_sig);
        }
        if let Some(info) = attrs.parts.get_mut(summary_part_name) {
            info.signature_sha1 = Some(summary_sig);
        }

        // Save the catalog.attrs file (after parts so signatures are present)
        let attrs_path = catalog_dir.join("catalog.attrs");
        debug!("Writing catalog.attrs to: {}", attrs_path.display());
        let _attrs_sig = catalog_writer::write_catalog_attrs(&attrs_path, &mut attrs)?;
        debug!("Wrote catalog.attrs file");

        // Create and save the update log if needed
        if create_update_log {
            debug!("Creating update log");
            let update_log_name = format!("update.{}Z.C", timestamp.split('.').next().unwrap());
            let update_log_path = catalog_dir.join(&update_log_name);
            debug!("Update log path: {}", update_log_path.display());

            let mut update_log = crate::repository::catalog::UpdateLog::new();
            debug!("Adding {} updates to the log", update_entries.len());
            for (fmri, catalog_parts, signature) in update_entries {
                update_log.add_update(
                    publisher,
                    &fmri,
                    crate::repository::catalog::CatalogOperationType::Add,
                    catalog_parts,
                    Some(signature),
                );
            }

            let _ = catalog_writer::write_update_log(&update_log_path, &mut update_log)?;
            debug!("Wrote update log file");

            // Add an update log to catalog.attrs
            debug!("Adding update log to catalog.attrs");
            attrs.updates.insert(
                update_log_name.clone(),
                crate::repository::catalog::UpdateLogInfo {
                    last_modified: timestamp.clone(),
                    signature_sha1: None,
                },
            );

            // Update the catalog.attrs file with the new update log
            debug!("Updating catalog.attrs file with new update log");
            let _ = catalog_writer::write_catalog_attrs(&attrs_path, &mut attrs)?;
            debug!("Updated catalog.attrs file");
        }

        info!("Catalog rebuilt for publisher: {}", publisher);
        Ok(())
    }

    /// Save an update log file to the publisher's catalog directory.
    ///
    /// The file name must follow the legacy pattern: `update.<logdate>.<locale>`
    /// for example: `update.20090524T042841Z.C`.
    pub fn save_update_log(
        &self,
        publisher: &str,
        log_filename: &str,
        log: &crate::repository::catalog::UpdateLog,
    ) -> Result<()> {
        if log_filename.contains('/') || log_filename.contains('\\') {
            return Err(RepositoryError::PathPrefixError(log_filename.to_string()));
        }

        // Ensure catalog dir exists
        let catalog_dir = Self::construct_catalog_path(&self.path, publisher);
        std::fs::create_dir_all(&catalog_dir).map_err(|e| RepositoryError::DirectoryCreateError { path: catalog_dir.clone(), source: e })?;

        // Serialize JSON
        let json = serde_json::to_vec_pretty(log)
            .map_err(|e| RepositoryError::JsonSerializeError(format!("Update log serialize error: {}", e)))?;

        // Write atomically
        let target = catalog_dir.join(log_filename);
        let tmp = target.with_extension("tmp");
        {
            let mut f = std::fs::File::create(&tmp)
                .map_err(|e| RepositoryError::FileWriteError { path: tmp.clone(), source: e })?;
            use std::io::Write as _;
            f.write_all(&json)
                .map_err(|e| RepositoryError::FileWriteError { path: tmp.clone(), source: e })?;
            f.flush().map_err(|e| RepositoryError::FileWriteError { path: tmp.clone(), source: e })?;
        }
        std::fs::rename(&tmp, &target)
            .map_err(|e| RepositoryError::FileWriteError { path: target.clone(), source: e })?;

        Ok(())
    }
    
    /// Generate the file path for a given hash using the new directory structure with publisher
    /// This is a wrapper around the construct_file_path_with_publisher helper method
    fn generate_file_path_with_publisher(&self, publisher: &str, hash: &str) -> PathBuf {
        Self::construct_file_path_with_publisher(&self.path, publisher, hash)
    }

    /// Get or initialize the catalog manager
    ///
    /// This method returns a mutable reference to the catalog manager.
    /// It uses interior mutability with RefCell to allow mutation through an immutable reference.
    /// 
    /// The catalog manager is specific to the given publisher.
    pub fn get_catalog_manager(
        &mut self,
        publisher: &str,
    ) -> Result<std::cell::RefMut<'_, crate::repository::catalog::CatalogManager>> {
        if self.catalog_manager.is_none() {
            let publisher_dir = self.path.join("publisher");
            let manager = crate::repository::catalog::CatalogManager::new(&publisher_dir, publisher)?;
            let refcell = std::cell::RefCell::new(manager);
            self.catalog_manager = Some(refcell);
        }

        // This is safe because we just checked that catalog_manager is Some
        Ok(self.catalog_manager.as_ref().unwrap().borrow_mut())
    }
    
    /// Get or initialize the obsoleted package manager
    ///
    /// This method returns a mutable reference to the obsoleted package manager.
    /// It uses interior mutability with RefCell to allow mutation through an immutable reference.
    pub fn get_obsoleted_manager(
        &mut self,
    ) -> Result<std::cell::RefMut<'_, crate::repository::obsoleted::ObsoletedPackageManager>> {
        if self.obsoleted_manager.is_none() {
            let manager = crate::repository::obsoleted::ObsoletedPackageManager::new(&self.path);
            let refcell = std::cell::RefCell::new(manager);
            self.obsoleted_manager = Some(refcell);
        }

        // This is safe because we just checked that obsoleted_manager is Some
        Ok(self.obsoleted_manager.as_ref().unwrap().borrow_mut())
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

    /// Build a search index for a publisher
    fn build_search_index(&self, publisher: &str) -> Result<()> {
        info!("Building search index for publisher: {}", publisher);

        // Create a new search index
        let mut index = SearchIndex::new();

        // Get the publisher's package directory
        let publisher_pkg_dir = Self::construct_package_dir(&self.path, publisher, "");

        // Check if the publisher directory exists
        if publisher_pkg_dir.exists() {
            // Use walkdir to recursively walk through the directory and process package manifests
            for entry in WalkDir::new(&publisher_pkg_dir)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                
                if path.is_file() {
                    // Try to read the first few bytes of the file to check if it's a manifest file
                    let mut file = match fs::File::open(&path) {
                        Ok(file) => file,
                        Err(err) => {
                            error!(
                                "FileBackend::build_search_index: Error opening file {}: {}",
                                path.display(),
                                err
                            );
                            continue;
                        }
                    };

                    let mut buffer = [0; 1024];
                    let bytes_read = match file.read(&mut buffer) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            error!(
                                "FileBackend::build_search_index: Error reading file {}: {}",
                                path.display(),
                                err
                            );
                            continue;
                        }
                    };

                    // Check if the file starts with a valid manifest marker
                    if bytes_read == 0
                        || (buffer[0] != b'{' && buffer[0] != b'<' && buffer[0] != b's')
                    {
                        continue;
                    }

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
                                                Some(
                                                    manifest
                                                        .files
                                                        .iter()
                                                        .map(|f| f.path.clone())
                                                        .collect(),
                                                )
                                            } else {
                                                None
                                            };

                                            let directories =
                                                if !manifest.directories.is_empty() {
                                                    Some(
                                                        manifest
                                                            .directories
                                                            .iter()
                                                            .map(|d| d.path.clone())
                                                            .collect(),
                                                    )
                                                } else {
                                                    None
                                                };

                                            let links = if !manifest.links.is_empty() {
                                                Some(
                                                    manifest
                                                        .links
                                                        .iter()
                                                        .map(|l| l.path.clone())
                                                        .collect(),
                                                )
                                            } else {
                                                None
                                            };

                                            let dependencies =
                                                if !manifest.dependencies.is_empty() {
                                                    Some(
                                                        manifest
                                                            .dependencies
                                                            .iter()
                                                            .filter_map(|d| {
                                                                d.fmri
                                                                    .as_ref()
                                                                    .map(|f| f.to_string())
                                                            })
                                                            .collect(),
                                                    )
                                                } else {
                                                    None
                                                };

                                            let licenses = if !manifest.licenses.is_empty() {
                                                Some(
                                                    manifest
                                                        .licenses
                                                        .iter()
                                                        .map(|l| {
                                                            if let Some(path_prop) =
                                                                l.properties.get("path")
                                                            {
                                                                path_prop.value.clone()
                                                            } else if let Some(license_prop) =
                                                                l.properties.get("license")
                                                            {
                                                                license_prop.value.clone()
                                                            } else {
                                                                l.payload.clone()
                                                            }
                                                        })
                                                        .collect(),
                                                )
                                            } else {
                                                None
                                            };

                                            // Create a PackageContents struct
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
                                        }
                                        Err(err) => {
                                            // Log the error but continue processing
                                            error!(
                                                "FileBackend::build_search_index: Error parsing FMRI '{}': {}",
                                                fmri_str, err
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            // Log the error but continue processing other files
                            error!(
                                "FileBackend::build_search_index: Error parsing manifest file {}: {}",
                                path.display(),
                                err
                            );
                        }
                    }
                }
            }
        }

        // Save the index to a file
        let index_path = self.path.join("index").join(publisher).join("search.json");
        index.save(&index_path)?;

        info!("Search index built for publisher: {}", publisher);

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
        debug!("Testing file publishing...");

        // Create a test publisher
        let publisher = "test";
        self.add_publisher(publisher)?;

        // Create a nested directory structure
        let nested_dir = test_dir.join("nested").join("dir");
        fs::create_dir_all(&nested_dir)?;

        // Create a test file in the nested directory
        let test_file_path = nested_dir.join("test_file.txt");
        fs::write(&test_file_path, "This is a test file")?;

        // Begin a transaction
        let mut transaction = self.begin_transaction()?;

        // Set the publisher for the transaction
        transaction.set_publisher(publisher);

        // Create a FileAction from the test file path
        let mut file_action = FileAction::read_from_path(&test_file_path)?;

        // Calculate the relative path from the test file path to the base directory
        let relative_path = test_file_path
            .strip_prefix(test_dir)?
            .to_string_lossy()
            .to_string();

        // Set the relative path in the FileAction
        file_action.path = relative_path;

        // Add the test file to the transaction
        transaction.add_file(file_action, &test_file_path)?;

        // Verify that the path in the FileAction is the relative path
        // The path should be "nested/dir/test_file.txt", not the full path
        let expected_path = "nested/dir/test_file.txt";
        let actual_path = &transaction.manifest.files[0].path;

        if actual_path != expected_path {
            return Err(RepositoryError::Other(format!(
                "Path in FileAction is incorrect. Expected: {}, Actual: {}",
                expected_path, actual_path
            )));
        }

        // Commit the transaction
        transaction.commit()?;

        // Verify the file was stored
        let hash = Transaction::calculate_file_hash(&test_file_path)?;
        // Use the new method with publisher
        let stored_file_path = self.generate_file_path_with_publisher(publisher, &hash);

        if !stored_file_path.exists() {
            return Err(RepositoryError::Other(
                "File was not stored correctly".to_string(),
            ));
        }

        // Verify the manifest was updated in the publisher-specific directory
        // The manifest should be named "unknown.manifest" since we didn't set a package name
        // Use the construct_package_dir helper to get the base directory, then join with the manifest name
        let pkg_dir = Self::construct_package_dir(&self.path, publisher, "unknown");
        let manifest_path = pkg_dir.join("manifest");

        if !manifest_path.exists() {
            return Err(RepositoryError::Other(format!(
                "Manifest was not created at the expected location: {}",
                manifest_path.display()
            )));
        }

        // Regenerate catalog and search index
        self.rebuild(Some(publisher), false, false)?;

        debug!("File publishing test passed!");

        Ok(())
    }

    /// Begin a new transaction for publishing
    pub fn begin_transaction(&self) -> Result<Transaction> {
        Transaction::new(self.path.clone())
    }

    /// Publish files from a prototype directory
    pub fn publish_files<P: AsRef<Path>>(&mut self, proto_dir: P, publisher: &str) -> Result<()> {
        let proto_dir = proto_dir.as_ref();

        // Check if the prototype directory exists
        if !proto_dir.exists() {
            return Err(RepositoryError::NotFound(format!(
                "Prototype directory does not exist: {}",
                proto_dir.display()
            )));
        }

        // Check if the publisher exists
        if !self.config.publishers.contains(&publisher.to_string()) {
            return Err(RepositoryError::PublisherNotFound(publisher.to_string()));
        }

        // Begin a transaction
        let mut transaction = self.begin_transaction()?;

        // Set the publisher for the transaction
        transaction.set_publisher(publisher);

        // Walk the prototype directory and add files to the transaction
        self.add_files_to_transaction(&mut transaction, proto_dir, proto_dir)?;

        // Commit the transaction
        transaction.commit()?;

        // Regenerate catalog and search index
        self.rebuild(Some(publisher), false, false)?;

        Ok(())
    }

    /// Add files from a directory to a transaction
    fn add_files_to_transaction(
        &self,
        transaction: &mut Transaction,
        base_dir: &Path,
        dir: &Path,
    ) -> Result<()> {
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
    pub fn store_file<P: AsRef<Path>>(&self, file_path: P, publisher: &str) -> Result<String> {
        let file_path = file_path.as_ref();

        // Calculate the SHA256 hash of the file
        let hash = Transaction::calculate_file_hash(file_path)?;

        // Create the destination path using the new directory structure with publisher
        let dest_path = self.generate_file_path_with_publisher(publisher, &hash);

        // Create parent directories if they don't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Copy the file if it doesn't already exist
        if !dest_path.exists() {
            fs::copy(file_path, &dest_path)?;
        }

        Ok(hash)
    }
}
