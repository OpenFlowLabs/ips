//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{debug, info, warn};

use reqwest::blocking::Client;
use serde_json::Value;

use super::{
    NoopProgressReporter, PackageContents, PackageInfo, ProgressInfo, ProgressReporter,
    PublisherInfo, ReadableRepository, RepositoryConfig, RepositoryError, RepositoryInfo,
    RepositoryVersion, Result, WritableRepository,
};
use super::catalog::CatalogManager;

/// Repository implementation that uses a REST API to interact with a remote repository.
///
/// This implementation allows downloading catalog files from a remote repository
/// and storing them locally for use by the client. It uses the existing `CatalogAttrs`
/// structure from catalog.rs to parse the downloaded catalog files.
///
/// # Example
///
/// ```no_run
/// use libips::repository::RestBackend;
/// use libips::repository::{ReadableRepository, WritableRepository};
/// use std::path::Path;
///
/// // Open a connection to a remote repository
/// let mut repo = RestBackend::open("https://pkg.opensolaris.org/release").unwrap();
///
/// // Set a local cache path for downloaded catalog files
/// repo.set_local_cache_path(Path::new("/tmp/pkg_cache")).unwrap();
///
/// // Add a publisher
/// repo.add_publisher("openindiana.org").unwrap();
///
/// // Download catalog files for the publisher
/// repo.download_catalog("openindiana.org", None).unwrap();
/// ```
pub struct RestBackend {
    /// The base URI of the repository
    pub uri: String,
    /// The repository configuration
    pub config: RepositoryConfig,
    /// The local path where catalog files are cached
    pub local_cache_path: Option<PathBuf>,
    /// HTTP client for making requests to the repository
    client: Client,
    /// Catalog managers for each publisher
    catalog_managers: HashMap<String, CatalogManager>,
}

impl WritableRepository for RestBackend {
    /// Create a new repository at the specified URI
    fn create<P: AsRef<Path>>(uri: P, version: RepositoryVersion) -> Result<Self> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to create the repository

        let uri_str = uri.as_ref().to_string_lossy().to_string();

        // Create the repository configuration
        let config = RepositoryConfig {
            version,
            ..Default::default()
        };

        // Create the repository structure
        let repo = RestBackend {
            uri: uri_str,
            config,
            local_cache_path: None,
            client: Client::new(),
            catalog_managers: HashMap::new(),
        };

        // In a real implementation, we would make a REST API call to create the repository structure

        Ok(repo)
    }

    /// Save the repository configuration
    fn save_config(&self) -> Result<()> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to save the repository configuration

        // For now, just return Ok
        Ok(())
    }

    /// Add a publisher to the repository
    fn add_publisher(&mut self, publisher: &str) -> Result<()> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to add the publisher

        println!("add_publisher called with publisher: {}", publisher);
        println!("Current publishers: {:?}", self.config.publishers);
        println!("Local cache path: {:?}", self.local_cache_path);

        // Add the publisher to the config if it doesn't exist
        if !self.config.publishers.contains(&publisher.to_string()) {
            self.config.publishers.push(publisher.to_string());
            println!("Publisher added to config: {:?}", self.config.publishers);

            // Save the updated configuration
            println!("Saving configuration...");
            match self.save_config() {
                Ok(_) => println!("Successfully saved configuration"),
                Err(e) => println!("Failed to save configuration: {}", e),
            }
        } else {
            println!("Publisher already exists in config, skipping addition to config");
        }

        // Always create the publisher directory if we have a local cache path
        // This ensures the directory exists even if the publisher was already in the config
        if let Some(cache_path) = &self.local_cache_path {
            println!("Creating publisher directory...");
            let publisher_dir = cache_path.join("publisher").join(publisher);
            println!("Publisher directory path: {}", publisher_dir.display());
            
            match fs::create_dir_all(&publisher_dir) {
                Ok(_) => println!("Successfully created publisher directory"),
                Err(e) => println!("Failed to create publisher directory: {}", e),
            }
            
            // Check if the directory was created
            println!("Publisher directory exists after creation: {}", publisher_dir.exists());
            
            // Create catalog directory
            let catalog_dir = publisher_dir.join("catalog");
            println!("Catalog directory path: {}", catalog_dir.display());
            
            match fs::create_dir_all(&catalog_dir) {
                Ok(_) => println!("Successfully created catalog directory"),
                Err(e) => println!("Failed to create catalog directory: {}", e),
            }
            
            // Check if the directory was created
            println!("Catalog directory exists after creation: {}", catalog_dir.exists());
            
            debug!("Created publisher directory: {}", publisher_dir.display());
        } else {
            println!("No local cache path set, skipping directory creation");
        }

        Ok(())
    }

    /// Remove a publisher from the repository
    fn remove_publisher(&mut self, publisher: &str, dry_run: bool) -> Result<()> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to remove the publisher

        if let Some(pos) = self.config.publishers.iter().position(|p| p == publisher) {
            if !dry_run {
                self.config.publishers.remove(pos);

                // In a real implementation, we would make a REST API call to remove publisher-specific resources

                // Save the updated configuration
                self.save_config()?;
            }
        }

        Ok(())
    }

    /// Set a repository property
    fn set_property(&mut self, property: &str, value: &str) -> Result<()> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to set the property

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
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to set the publisher property

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
        // This is a stub implementation
        // In a real implementation; we would make a REST API call to rebuild metadata

        // Filter publishers if specified
        let publishers = if let Some(pub_name) = publisher {
            if !self.config.publishers.contains(&pub_name.to_string()) {
                return Err(RepositoryError::PublisherNotFound(pub_name.to_string()));
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
                // In a real implementation, we would make a REST API call to rebuild the catalog
            }

            if !no_index {
                println!("Rebuilding search index...");
                // In a real implementation, we would make a REST API call to rebuild the search index
            }
        }

        Ok(())
    }

    /// Refresh repository metadata
    fn refresh(&self, publisher: Option<&str>, no_catalog: bool, no_index: bool) -> Result<()> {
        // We need to clone self to avoid borrowing issues
        let mut cloned_self = RestBackend {
            uri: self.uri.clone(),
            config: self.config.clone(),
            local_cache_path: self.local_cache_path.clone(),
            client: Client::new(),
            catalog_managers: HashMap::new(),
        };
        
        // Check if we have a local cache path
        if cloned_self.local_cache_path.is_none() {
            return Err(RepositoryError::Other("No local cache path set".to_string()));
        }

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
                // Download the catalog files
                cloned_self.download_catalog(&pub_name, None)?;
            }

            if !no_index {
                info!("Refreshing search index...");
                // In a real implementation, we would refresh the search index
                // This would typically involve parsing the catalog files and building an index
            }
        }

        Ok(())
    }

    /// Set the default publisher for the repository
    fn set_default_publisher(&mut self, publisher: &str) -> Result<()> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to set the default publisher

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

impl ReadableRepository for RestBackend {
    /// Open an existing repository
    fn open<P: AsRef<Path>>(uri: P) -> Result<Self> {
        let uri_str = uri.as_ref().to_string_lossy().to_string();
        
        // Create an HTTP client
        let client = Client::new();
        
        // Fetch the repository configuration from the remote server
        // We'll try to get the publisher information using the publisher endpoint
        let url = format!("{}/publisher/0", uri_str);
        
        debug!("Fetching repository configuration from: {}", url);
        
        let mut config = RepositoryConfig::default();
        
        // Try to fetch publisher information
        match client.get(&url).send() {
            Ok(response) => {
                if response.status().is_success() {
                    // Try to parse the response as JSON
                    match response.json::<Value>() {
                        Ok(json) => {
                            // Extract publisher information
                            if let Some(publishers) = json.get("publishers").and_then(|p| p.as_object()) {
                                for (name, _) in publishers {
                                    debug!("Found publisher: {}", name);
                                    config.publishers.push(name.clone());
                                }
                            }
                        },
                        Err(e) => {
                            warn!("Failed to parse publisher information: {}", e);
                        }
                    }
                } else {
                    warn!("Failed to fetch publisher information: HTTP status {}", response.status());
                }
            },
            Err(e) => {
                warn!("Failed to connect to repository: {}", e);
            }
        }
        
        // If we couldn't get any publishers, add a default one
        if config.publishers.is_empty() {
            config.publishers.push("openindiana.org".to_string());
        }
        
        // Create the repository instance
        Ok(RestBackend {
            uri: uri_str,
            config,
            local_cache_path: None,
            client,
            catalog_managers: HashMap::new(),
        })
    }

    /// Get repository information
    fn get_info(&self) -> Result<RepositoryInfo> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to get repository information

        let mut publishers = Vec::new();

        for publisher_name in &self.config.publishers {
            // In a real implementation, we would get this information from the REST API
            let package_count = 0;
            let status = "online".to_string();
            let updated = "2025-07-21T18:46:00.000000Z".to_string();

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
        _pattern: Option<&str>,
    ) -> Result<Vec<PackageInfo>> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to list packages

        let packages = Vec::new();

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
        for _pub_name in publishers {
            // In a real implementation, we would make a REST API call to get package information
            // The API call would return a list of packages with their names, versions, and other metadata
            // We would then parse this information and create PackageInfo structs

            // For now, we return an empty list since we don't want to return placeholder data
            // and we don't have a real API to call

            // If pattern filtering is needed, it would be applied here to the results from the API
            // When implementing, use the regex crate to handle user-provided regexp patterns properly,
            // similar to the implementation in file_backend.rs
        }

        Ok(packages)
    }

    /// Show contents of packages
    fn show_contents(
        &self,
        publisher: Option<&str>,
        pattern: Option<&str>,
        action_types: Option<&[String]>,
    ) -> Result<Vec<PackageContents>> {
        // This is a stub implementation
        // In a real implementation, we would make a REST API call to get package contents

        // Get the list of packages
        let packages = self.list_packages(publisher, pattern)?;

        // For each package, create a PackageContents struct
        let mut package_contents = Vec::new();

        for pkg_info in packages {
            // In a real implementation, we would get this information from the REST API

            // Format the package identifier using the FMRI
            let version = pkg_info.fmri.version();
            let pkg_id = if !version.is_empty() {
                format!("{}@{}", pkg_info.fmri.stem(), version)
            } else {
                pkg_info.fmri.stem().to_string()
            };

            // Example content for each type
            // In a real implementation, we would get this information from the REST API

            // Files
            let files = if action_types.is_none()
                || action_types.as_ref().unwrap().contains(&"file".to_string())
            {
                Some(vec![
                    "/usr/bin/example".to_string(),
                    "/usr/lib/example.so".to_string(),
                ])
            } else {
                None
            };

            // Directories
            let directories = if action_types.is_none()
                || action_types.as_ref().unwrap().contains(&"dir".to_string())
            {
                Some(vec![
                    "/usr/share/doc/example".to_string(),
                    "/usr/share/man/man1".to_string(),
                ])
            } else {
                None
            };

            // Links
            let links = if action_types.is_none()
                || action_types.as_ref().unwrap().contains(&"link".to_string())
            {
                Some(vec!["/usr/bin/example-link".to_string()])
            } else {
                None
            };

            // Dependencies
            let dependencies = if action_types.is_none()
                || action_types
                    .as_ref()
                    .unwrap()
                    .contains(&"depend".to_string())
            {
                Some(vec!["pkg:/system/library@0.5.11".to_string()])
            } else {
                None
            };

            // Licenses
            let licenses = if action_types.is_none()
                || action_types
                    .as_ref()
                    .unwrap()
                    .contains(&"license".to_string())
            {
                Some(vec!["/usr/share/licenses/example/LICENSE".to_string()])
            } else {
                None
            };

            // Add the package contents to the result
            package_contents.push(PackageContents {
                package_id: pkg_id,
                files,
                directories,
                links,
                dependencies,
                licenses,
            });
        }

        Ok(package_contents)
    }

    fn fetch_payload(
        &mut self,
        publisher: &str,
        digest: &str,
        dest: &Path,
    ) -> Result<()> {
        // Determine hash and algorithm from the provided digest string
        let mut hash = digest.to_string();
        let mut algo: Option<crate::digest::DigestAlgorithm> = None;
        if digest.contains(':') {
            if let Ok(d) = crate::digest::Digest::from_str(digest) {
                hash = d.hash.clone();
                algo = Some(d.algorithm);
            }
        }

        if hash.is_empty() {
            return Err(RepositoryError::Other("Empty digest provided".to_string()));
        }

        let shard = if hash.len() >= 2 { &hash[0..2] } else { &hash[..] };
        let candidates = vec![
            format!("{}/file/{}/{}", self.uri, shard, hash),
            format!("{}/publisher/{}/file/{}/{}", self.uri, publisher, shard, hash),
        ];

        // Ensure destination directory exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut last_err: Option<String> = None;
        for url in candidates {
            match self.client.get(&url).send() {
                Ok(resp) if resp.status().is_success() => {
                    let body = resp.bytes().map_err(|e| RepositoryError::Other(format!("Failed to read payload body: {}", e)))?;

                    // Verify digest if algorithm is known
                    if let Some(alg) = algo.clone() {
                        match crate::digest::Digest::from_bytes(&body, alg, crate::digest::DigestSource::PrimaryPayloadHash) {
                            Ok(comp) => {
                                if comp.hash != hash {
                                    return Err(RepositoryError::DigestError(format!(
                                        "Digest mismatch: expected {}, got {}",
                                        hash, comp.hash
                                    )));
                                }
                            }
                            Err(e) => return Err(RepositoryError::DigestError(format!("{}", e))),
                        }
                    }

                    // Write atomically
                    let tmp = dest.with_extension("tmp");
                    let mut f = File::create(&tmp)?;
                    f.write_all(&body)?;
                    drop(f);
                    fs::rename(&tmp, dest)?;
                    return Ok(());
                }
                Ok(resp) => {
                    last_err = Some(format!("HTTP {} for {}", resp.status(), url));
                }
                Err(e) => {
                    last_err = Some(format!("{} for {}", e, url));
                }
            }
        }

        Err(RepositoryError::NotFound(last_err.unwrap_or_else(|| "payload not found".to_string())))
    }

    fn fetch_manifest(
        &mut self,
        publisher: &str,
        fmri: &crate::fmri::Fmri,
    ) -> Result<crate::actions::Manifest> {
        let text = self.fetch_manifest_text(publisher, fmri)?;
        crate::actions::Manifest::parse_string(text).map_err(RepositoryError::from)
    }

    fn search(
        &self,
        _query: &str,
        _publisher: Option<&str>,
        _limit: Option<usize>,
    ) -> Result<Vec<PackageInfo>> {
        todo!()
    }
}

impl RestBackend {
    pub fn fetch_manifest_text(
        &mut self,
        publisher: &str,
        fmri: &crate::fmri::Fmri,
    ) -> Result<String> {
        // Require versioned FMRI
        let version = fmri.version();
        if version.is_empty() {
            return Err(RepositoryError::Other("FMRI must include a version to fetch manifest".into()));
        }
        // URL-encode helper
        let url_encode = |s: &str| -> String {
            let mut out = String::new();
            for b in s.bytes() {
                match b {
                    b'-' | b'_' | b'.' | b'~' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => out.push(b as char),
                    b' ' => out.push('+'),
                    _ => {
                        out.push('%');
                        out.push_str(&format!("{:02X}", b));
                    }
                }
            }
            out
        };
        let encoded_fmri = url_encode(&format!("{}@{}", fmri.stem(), version));
        let encoded_stem = url_encode(fmri.stem());
        let encoded_version = url_encode(&version);
        let candidates = vec![
            format!("{}/manifest/0/{}", self.uri, encoded_fmri),
            format!("{}/publisher/{}/manifest/0/{}", self.uri, publisher, encoded_fmri),
            // Fallbacks to direct file-style paths if server exposes static files
            format!("{}/pkg/{}/{}", self.uri, encoded_stem, encoded_version),
            format!("{}/publisher/{}/pkg/{}/{}", self.uri, publisher, encoded_stem, encoded_version),
        ];
        let mut last_err: Option<String> = None;
        for url in candidates {
            match self.client.get(&url).send() {
                Ok(resp) if resp.status().is_success() => {
                    let text = resp.text().map_err(|e| RepositoryError::Other(format!("Failed to read manifest body: {}", e)))?;
                    return Ok(text);
                }
                Ok(resp) => {
                    last_err = Some(format!("HTTP {} for {}", resp.status(), url));
                }
                Err(e) => {
                    last_err = Some(format!("{} for {}", e, url));
                }
            }
        }
        Err(RepositoryError::NotFound(last_err.unwrap_or_else(|| "manifest not found".to_string())))
    }
    /// Sets the local path where catalog files will be cached.
    ///
    /// This method creates the directory if it doesn't exist. The local cache path
    /// is required for downloading and storing catalog files from the remote repository.
    ///
    /// # Arguments
    ///
    /// * `path` - The path where catalog files will be stored
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok if the path was set successfully, Err otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if the directory could not be created.
    pub fn set_local_cache_path<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.local_cache_path = Some(path.as_ref().to_path_buf());
        
        // Create the directory if it doesn't exist
        if let Some(path) = &self.local_cache_path {
            fs::create_dir_all(path)?;
        }
        
        Ok(())
    }
    
    /// Initializes the repository by downloading catalog files for all publishers.
    ///
    /// This method should be called after setting the local cache path with
    /// `set_local_cache_path`. It downloads the catalog files for all publishers
    /// in the repository configuration.
    ///
    /// # Arguments
    ///
    /// * `progress` - Optional progress reporter for tracking download progress
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok if initialization was successful, Err otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No local cache path has been set
    /// - Failed to download catalog files for any publisher
    pub fn initialize(&mut self, progress: Option<&dyn ProgressReporter>) -> Result<()> {
        // Check if we have a local cache path
        if self.local_cache_path.is_none() {
            return Err(RepositoryError::Other("No local cache path set".to_string()));
        }
        
        // Download catalogs for all publishers
        self.download_all_catalogs(progress)?;
        
        Ok(())
    }
    
    /// Get the catalog manager for a publisher
    fn get_catalog_manager(&mut self, publisher: &str) -> Result<&mut CatalogManager> {
        // Check if we have a local cache path
        let cache_path = match &self.local_cache_path {
            Some(path) => path,
            None => return Err(RepositoryError::Other("No local cache path set".to_string())),
        };

        // The local cache path is expected to already point to the per-publisher directory
        // Ensure the directory exists
        fs::create_dir_all(cache_path)?;

        // Get or create the catalog manager pointing at the per-publisher directory directly
        if !self.catalog_managers.contains_key(publisher) {
            let catalog_manager = CatalogManager::new(cache_path, publisher)?;
            self.catalog_managers.insert(publisher.to_string(), catalog_manager);
        }

        Ok(self.catalog_managers.get_mut(publisher).unwrap())
    }
    
    /// Downloads a catalog file from the remote server.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The name of the publisher
    /// * `file_name` - The name of the catalog file to download
    /// * `progress` - Optional progress reporter for tracking download progress
    ///
    /// # Returns
    ///
    /// * `Result<Vec<u8>>` - The content of the downloaded file if successful
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to connect to the remote server
    /// - The HTTP request was not successful
    /// - Failed to read the response body
    fn download_catalog_file(
        &self,
        publisher: &str,
        file_name: &str,
        progress: Option<&dyn ProgressReporter>,
    ) -> Result<Vec<u8>> {
        // Use a no-op reporter if none was provided
        let progress = progress.unwrap_or(&NoopProgressReporter);

        // Prepare candidate URLs to support both modern and legacy pkg5 depotd layouts
        let mut urls: Vec<String> = vec![
            format!("{}/catalog/1/{}", self.uri, file_name),
            format!("{}/publisher/{}/catalog/1/{}", self.uri, publisher, file_name),
        ];
        if file_name == "catalog.attrs" {
            // Some older depots expose catalog.attrs at the root or under publisher path
            urls.insert(1, format!("{}/catalog.attrs", self.uri));
            urls.push(format!("{}/publisher/{}/catalog.attrs", self.uri, publisher));
        }

        debug!(
            "Attempting to download '{}' via {} candidate URL(s)",
            file_name,
            urls.len()
        );

        // Create progress info for this operation
        let mut progress_info = ProgressInfo::new(format!("Downloading {}", file_name))
            .with_context(format!("Publisher: {}", publisher));

        // Notify that we're starting the download
        progress.start(&progress_info);

        let mut last_error: Option<String> = None;

        for url in urls {
            debug!("Trying URL: {}", url);
            match self.client.get(&url).send() {
                Ok(resp) => {
                    if resp.status().is_success() {
                        // Update total if server provided content length
                        if let Some(content_length) = resp.content_length() {
                            progress_info = progress_info.with_total(content_length);
                            progress.update(&progress_info);
                        }

                        // Read the response body
                        let body = resp.bytes().map_err(|e| {
                            progress.finish(&progress_info);
                            RepositoryError::Other(format!("Failed to read response body: {}", e))
                        })?;

                        // Update progress with the final size
                        progress_info = progress_info.with_current(body.len() as u64);
                        if progress_info.total.is_none() {
                            progress_info = progress_info.with_total(body.len() as u64);
                        }

                        // Report completion
                        progress.finish(&progress_info);
                        return Ok(body.to_vec());
                    } else {
                        last_error = Some(format!("HTTP status {} for {}", resp.status(), url));
                    }
                }
                Err(e) => {
                    last_error = Some(format!("{} for {}", e, url));
                }
            }
        }

        // Report failure after exhausting all URLs
        progress.finish(&progress_info);
        Err(RepositoryError::Other(match last_error {
            Some(s) => format!(
                "Failed to download '{}' from any known endpoint: {}",
                file_name, s
            ),
            None => format!(
                "Failed to download '{}' from any known endpoint",
                file_name
            ),
        }))
    }
    
    /// Download and store a catalog file
    ///
    /// # Arguments
    ///
    /// * `publisher` - The name of the publisher
    /// * `file_name` - The name of the catalog file to download
    /// * `progress` - Optional progress reporter for tracking download progress
    ///
    /// # Returns
    ///
    /// * `Result<PathBuf>` - The path to the stored file if successful
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No local cache path has been set
    /// - Failed to create the publisher or catalog directory
    /// - Failed to download the catalog file
    /// - Failed to create or write to the file
    fn download_and_store_catalog_file(
        &mut self,
        publisher: &str,
        file_name: &str,
        progress: Option<&dyn ProgressReporter>,
    ) -> Result<PathBuf> {
        // Check if we have a local cache path
        let cache_path = match &self.local_cache_path {
            Some(path) => path,
            None => return Err(RepositoryError::Other("No local cache path set".to_string())),
        };

        // Ensure the per-publisher directory (local cache path) exists
        fs::create_dir_all(cache_path)?;

        // Download the catalog file
        let content = self.download_catalog_file(publisher, file_name, progress)?;

        // Use a no-op reporter if none was provided
        let progress = progress.unwrap_or(&NoopProgressReporter);

        // Create progress info for storing the file
        let progress_info = ProgressInfo::new(format!("Storing {}", file_name))
            .with_context(format!("Publisher: {}", publisher))
            .with_current(0)
            .with_total(content.len() as u64);

        // Notify that we're starting to store the file
        progress.start(&progress_info);

        // Store the file directly under the per-publisher directory
        let file_path = cache_path.join(file_name);
        let mut file = File::create(&file_path)
            .map_err(|e| {
                // Report failure
                progress.finish(&progress_info);
                RepositoryError::FileWriteError { path: file_path.clone(), source: e }
            })?;

        file.write_all(&content)
            .map_err(|e| {
                // Report failure
                progress.finish(&progress_info);
                RepositoryError::FileWriteError { path: file_path.clone(), source: e }
            })?;

        debug!("Stored catalog file: {}", file_path.display());

        // Report completion
        let progress_info = progress_info.with_current(content.len() as u64);
        progress.finish(&progress_info);

        Ok(file_path)
    }
    
    /// Downloads all catalog files for a specific publisher.
    ///
    /// This method downloads the catalog.attrs file first to determine what catalog parts
    /// are available, then downloads each part and loads them into the catalog manager.
    /// It uses the existing `CatalogAttrs` structure from catalog.rs to parse the
    /// downloaded catalog files.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The name of the publisher to download catalog files for
    /// * `progress` - Optional progress reporter for tracking download progress
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok if all catalog files were downloaded successfully, Err otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No local cache path has been set
    /// - Failed to download the catalog.attrs file
    /// - Failed to parse the catalog.attrs file
    /// - Failed to download any catalog part
    /// - Failed to load any catalog part into the catalog manager
    pub fn download_catalog(
        &mut self,
        publisher: &str,
        progress: Option<&dyn ProgressReporter>,
    ) -> Result<()> {
        // Use a no-op reporter if none was provided
        let progress_reporter = progress.unwrap_or(&NoopProgressReporter);
        
        // Create progress info for the overall operation
        let mut overall_progress = ProgressInfo::new(format!("Downloading catalog for {}", publisher));
        
        // Notify that we're starting the download
        progress_reporter.start(&overall_progress);
        
        // First download catalog.attrs to get the list of available parts
        let attrs_path = self.download_and_store_catalog_file(publisher, "catalog.attrs", progress)?;
        
        // Parse the catalog.attrs file to get the list of parts
        let attrs_content = fs::read_to_string(&attrs_path)
            .map_err(|e| {
                progress_reporter.finish(&overall_progress);
                RepositoryError::FileReadError { path: attrs_path.clone(), source: e }
            })?;
        
        let attrs: Value = serde_json::from_str(&attrs_content)
            .map_err(|e| {
                progress_reporter.finish(&overall_progress);
                RepositoryError::JsonParseError(format!("Failed to parse catalog.attrs: {}", e))
            })?;
        
        // Get the list of parts
        let parts = attrs["parts"].as_object().ok_or_else(|| {
            progress_reporter.finish(&overall_progress);
            RepositoryError::JsonParseError("Missing 'parts' field in catalog.attrs".to_string())
        })?;
        
        // Update progress with total number of parts
        let total_parts = parts.len() as u64 + 1; // +1 for catalog.attrs
        overall_progress = overall_progress.with_total(total_parts).with_current(1);
        progress_reporter.update(&overall_progress);
        
        // Download each part
        for (i, part_name) in parts.keys().enumerate() {
            debug!("Downloading catalog part: {}", part_name);
            
            // Update progress with current part
            overall_progress = overall_progress.with_current(i as u64 + 2) // +2 because we already downloaded catalog.attrs
                .with_context(format!("Downloading part: {}", part_name));
            progress_reporter.update(&overall_progress);
            
            self.download_and_store_catalog_file(publisher, part_name, progress)?;
        }
        
        // Get the catalog manager for this publisher
        let catalog_manager = self.get_catalog_manager(publisher)?;
        
        // Update progress for loading parts
        overall_progress = overall_progress.with_context("Loading catalog parts".to_string());
        progress_reporter.update(&overall_progress);
        
        // Load the catalog parts
        for part_name in parts.keys() {
            catalog_manager.load_part(part_name)?;
        }
        
        // Report completion
        overall_progress = overall_progress.with_current(total_parts);
        progress_reporter.finish(&overall_progress);
        
        info!("Downloaded catalog for publisher: {}", publisher);
        
        Ok(())
    }
    
    /// Download catalogs for all publishers
    ///
    /// # Arguments
    ///
    /// * `progress` - Optional progress reporter for tracking download progress
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok if all catalogs were downloaded successfully, Err otherwise
    pub fn download_all_catalogs(&mut self, progress: Option<&dyn ProgressReporter>) -> Result<()> {
        // Use a no-op reporter if none was provided
        let progress_reporter = progress.unwrap_or(&NoopProgressReporter);
        
        // Clone the publishers list to avoid borrowing issues
        let publishers = self.config.publishers.clone();
        let total_publishers = publishers.len() as u64;
        
        // Create progress info for the overall operation
        let mut overall_progress = ProgressInfo::new("Downloading all catalogs")
            .with_total(total_publishers)
            .with_current(0);
        
        // Notify that we're starting the download
        progress_reporter.start(&overall_progress);
        
        // Download catalogs for each publisher
        for (i, publisher) in publishers.iter().enumerate() {
            // Update progress with current publisher
            overall_progress = overall_progress
                .with_current(i as u64)
                .with_context(format!("Publisher: {}", publisher));
            progress_reporter.update(&overall_progress);
            
            // Download catalog for this publisher
            self.download_catalog(publisher, progress)?;
            
            // Update progress after completing this publisher
            overall_progress = overall_progress.with_current(i as u64 + 1);
            progress_reporter.update(&overall_progress);
        }
        
        // Report completion
        progress_reporter.finish(&overall_progress);
        
        Ok(())
    }
    
    /// Refresh the catalog for a publisher
    ///
    /// # Arguments
    ///
    /// * `publisher` - The name of the publisher to refresh
    /// * `progress` - Optional progress reporter for tracking download progress
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok if the catalog was refreshed successfully, Err otherwise
    pub fn refresh_catalog(&mut self, publisher: &str, progress: Option<&dyn ProgressReporter>) -> Result<()> {
        self.download_catalog(publisher, progress)
    }
}
