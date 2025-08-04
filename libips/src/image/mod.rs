mod properties;
#[cfg(test)]
mod tests;

use miette::Diagnostic;
use properties::*;
use redb::Database;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::repository::{ReadableRepository, RepositoryError, RestBackend};

// Export the catalog module
pub mod catalog;
use catalog::{ImageCatalog, PackageInfo};

// Export the installed packages module
pub mod installed;
use installed::{InstalledPackageInfo, InstalledPackages};

// Include tests
#[cfg(test)]
mod installed_tests;

#[derive(Debug, Error, Diagnostic)]
pub enum ImageError {
    #[error("I/O error: {0}")]
    #[diagnostic(
        code(ips::image_error::io),
        help("Check system resources and permissions")
    )]
    IO(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    #[diagnostic(
        code(ips::image_error::json),
        help("Check the JSON format and try again")
    )]
    Json(#[from] serde_json::Error),

    #[error("Invalid image path: {0}")]
    #[diagnostic(
        code(ips::image_error::invalid_path),
        help("Provide a valid path for the image")
    )]
    InvalidPath(String),
    
    #[error("Repository error: {0}")]
    #[diagnostic(
        code(ips::image_error::repository),
        help("Check the repository configuration and try again")
    )]
    Repository(#[from] RepositoryError),
    
    #[error("Database error: {0}")]
    #[diagnostic(
        code(ips::image_error::database),
        help("Check the database configuration and try again")
    )]
    Database(String),
    
    #[error("Publisher not found: {0}")]
    #[diagnostic(
        code(ips::image_error::publisher_not_found),
        help("Check the publisher name and try again")
    )]
    PublisherNotFound(String),
    
    #[error("No publishers configured")]
    #[diagnostic(
        code(ips::image_error::no_publishers),
        help("Configure at least one publisher before performing this operation")
    )]
    NoPublishers,
}

pub type Result<T> = std::result::Result<T, ImageError>;

/// Type of image, either Full (base path of "/") or Partial (attached to a full image)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum ImageType {
    /// Full image with base path of "/"
    Full,
    /// Partial image attached to a full image
    Partial,
}

/// Represents a publisher configuration in an image
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Publisher {
    /// Publisher name
    pub name: String,
    /// Publisher origin URL
    pub origin: String,
    /// Publisher mirror URLs
    pub mirrors: Vec<String>,
    /// Whether this is the default publisher
    pub is_default: bool,
}

/// Represents an IPS image, which can be either a Full image or a Partial image
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Image {
    /// Path to the image
    path: PathBuf,
    /// Type of image (Full or Partial)
    image_type: ImageType,
    /// Image properties
    props: Vec<ImageProperty>,
    /// Image version
    version: i32,
    /// Variants
    variants: HashMap<String, String>,
    /// Mediators
    mediators: HashMap<String, String>,
    /// Publishers
    publishers: Vec<Publisher>,
}

impl Image {
    /// Creates a new Full image at the specified path
    pub fn new_full<P: Into<PathBuf>>(path: P) -> Image {
        Image {
            path: path.into(),
            image_type: ImageType::Full,
            version: 5,
            variants: HashMap::new(),
            mediators: HashMap::new(),
            props: vec![],
            publishers: vec![],
        }
    }

    /// Creates a new Partial image at the specified path
    pub fn new_partial<P: Into<PathBuf>>(path: P) -> Image {
        Image {
            path: path.into(),
            image_type: ImageType::Partial,
            version: 5,
            variants: HashMap::new(),
            mediators: HashMap::new(),
            props: vec![],
            publishers: vec![],
        }
    }
    
    /// Add a publisher to the image
    pub fn add_publisher(&mut self, name: &str, origin: &str, mirrors: Vec<String>, is_default: bool) -> Result<()> {
        // Check if publisher already exists
        if self.publishers.iter().any(|p| p.name == name) {
            // Update existing publisher
            for publisher in &mut self.publishers {
                if publisher.name == name {
                    publisher.origin = origin.to_string();
                    publisher.mirrors = mirrors;
                    publisher.is_default = is_default;
                    
                    // If this publisher is now the default, make sure no other publisher is default
                    if is_default {
                        for other_publisher in &mut self.publishers {
                            if other_publisher.name != name {
                                other_publisher.is_default = false;
                            }
                        }
                    }
                    
                    break;
                }
            }
        } else {
            // Add new publisher
            let publisher = Publisher {
                name: name.to_string(),
                origin: origin.to_string(),
                mirrors,
                is_default,
            };
            
            // If this publisher is the default, make sure no other publisher is default
            if is_default {
                for publisher in &mut self.publishers {
                    publisher.is_default = false;
                }
            }
            
            self.publishers.push(publisher);
        }
        
        // Save the image to persist the changes
        self.save()?;
        
        Ok(())
    }
    
    /// Remove a publisher from the image
    pub fn remove_publisher(&mut self, name: &str) -> Result<()> {
        let initial_len = self.publishers.len();
        self.publishers.retain(|p| p.name != name);
        
        if self.publishers.len() == initial_len {
            return Err(ImageError::PublisherNotFound(name.to_string()));
        }
        
        // If we removed the default publisher, set the first remaining publisher as default
        if self.publishers.iter().all(|p| !p.is_default) && !self.publishers.is_empty() {
            self.publishers[0].is_default = true;
        }
        
        // Save the image to persist the changes
        self.save()?;
        
        Ok(())
    }
    
    /// Get the default publisher
    pub fn default_publisher(&self) -> Result<&Publisher> {
        // Find the default publisher
        for publisher in &self.publishers {
            if publisher.is_default {
                return Ok(publisher);
            }
        }
        
        // If no publisher is marked as default, return the first one
        if !self.publishers.is_empty() {
            return Ok(&self.publishers[0]);
        }
        
        Err(ImageError::NoPublishers)
    }
    
    /// Get a publisher by name
    pub fn get_publisher(&self, name: &str) -> Result<&Publisher> {
        for publisher in &self.publishers {
            if publisher.name == name {
                return Ok(publisher);
            }
        }
        
        Err(ImageError::PublisherNotFound(name.to_string()))
    }
    
    /// Get all publishers
    pub fn publishers(&self) -> &[Publisher] {
        &self.publishers
    }

    /// Returns the path to the image
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the type of the image
    pub fn image_type(&self) -> &ImageType {
        &self.image_type
    }

    /// Returns the path to the metadata directory for this image
    pub fn metadata_dir(&self) -> PathBuf {
        match self.image_type {
            ImageType::Full => self.path.join("var/pkg"),
            ImageType::Partial => self.path.join(".pkg"),
        }
    }

    /// Returns the path to the image JSON file
    pub fn image_json_path(&self) -> PathBuf {
        self.metadata_dir().join("pkg6.image.json")
    }
    
    /// Returns the path to the installed packages database
    pub fn installed_db_path(&self) -> PathBuf {
        self.metadata_dir().join("installed.redb")
    }
    
    /// Returns the path to the manifest directory
    pub fn manifest_dir(&self) -> PathBuf {
        self.metadata_dir().join("manifests")
    }
    
    /// Returns the path to the catalog directory
    pub fn catalog_dir(&self) -> PathBuf {
        self.metadata_dir().join("catalog")
    }
    
    /// Returns the path to the catalog database
    pub fn catalog_db_path(&self) -> PathBuf {
        self.metadata_dir().join("catalog.redb")
    }

    /// Creates the metadata directory if it doesn't exist
    pub fn create_metadata_dir(&self) -> Result<()> {
        let metadata_dir = self.metadata_dir();
        fs::create_dir_all(&metadata_dir).map_err(|e| {
            ImageError::IO(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create metadata directory at {:?}: {}", metadata_dir, e),
            ))
        })
    }
    
    /// Creates the manifest directory if it doesn't exist
    pub fn create_manifest_dir(&self) -> Result<()> {
        let manifest_dir = self.manifest_dir();
        fs::create_dir_all(&manifest_dir).map_err(|e| {
            ImageError::IO(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create manifest directory at {:?}: {}", manifest_dir, e),
            ))
        })
    }
    
    /// Creates the catalog directory if it doesn't exist
    pub fn create_catalog_dir(&self) -> Result<()> {
        let catalog_dir = self.catalog_dir();
        fs::create_dir_all(&catalog_dir).map_err(|e| {
            ImageError::IO(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create catalog directory at {:?}: {}", catalog_dir, e),
            ))
        })
    }
    
    /// Initialize the installed packages database
    pub fn init_installed_db(&self) -> Result<()> {
        let db_path = self.installed_db_path();
        
        // Create the installed packages database
        let installed = InstalledPackages::new(&db_path);
        installed.init_db().map_err(|e| {
            ImageError::Database(format!("Failed to initialize installed packages database: {}", e))
        })
    }
    
    /// Add a package to the installed packages database
    pub fn install_package(&self, fmri: &crate::fmri::Fmri, manifest: &crate::actions::Manifest) -> Result<()> {
        let installed = InstalledPackages::new(self.installed_db_path());
        installed.add_package(fmri, manifest).map_err(|e| {
            ImageError::Database(format!("Failed to add package to installed database: {}", e))
        })
    }
    
    /// Remove a package from the installed packages database
    pub fn uninstall_package(&self, fmri: &crate::fmri::Fmri) -> Result<()> {
        let installed = InstalledPackages::new(self.installed_db_path());
        installed.remove_package(fmri).map_err(|e| {
            ImageError::Database(format!("Failed to remove package from installed database: {}", e))
        })
    }
    
    /// Query the installed packages database for packages matching a pattern
    pub fn query_installed_packages(&self, pattern: Option<&str>) -> Result<Vec<InstalledPackageInfo>> {
        let installed = InstalledPackages::new(self.installed_db_path());
        installed.query_packages(pattern).map_err(|e| {
            ImageError::Database(format!("Failed to query installed packages: {}", e))
        })
    }
    
    /// Get a manifest from the installed packages database
    pub fn get_manifest_from_installed(&self, fmri: &crate::fmri::Fmri) -> Result<Option<crate::actions::Manifest>> {
        let installed = InstalledPackages::new(self.installed_db_path());
        installed.get_manifest(fmri).map_err(|e| {
            ImageError::Database(format!("Failed to get manifest from installed database: {}", e))
        })
    }
    
    /// Check if a package is installed
    pub fn is_package_installed(&self, fmri: &crate::fmri::Fmri) -> Result<bool> {
        let installed = InstalledPackages::new(self.installed_db_path());
        installed.is_installed(fmri).map_err(|e| {
            ImageError::Database(format!("Failed to check if package is installed: {}", e))
        })
    }
    
    /// Initialize the catalog database
    pub fn init_catalog_db(&self) -> Result<()> {
        let catalog = ImageCatalog::new(self.catalog_dir(), self.catalog_db_path());
        catalog.init_db().map_err(|e| {
            ImageError::Database(format!("Failed to initialize catalog database: {}", e))
        })
    }
    
    /// Download catalogs from all configured publishers and build the merged catalog
    pub fn download_catalogs(&self) -> Result<()> {
        // Create catalog directory if it doesn't exist
        self.create_catalog_dir()?;
        
        // Download catalogs for each publisher
        for publisher in &self.publishers {
            self.download_publisher_catalog(&publisher.name)?;
        }
        
        // Build the merged catalog
        self.build_catalog()?;
        
        Ok(())
    }
    
    /// Build the merged catalog from downloaded catalogs
    pub fn build_catalog(&self) -> Result<()> {
        // Initialize the catalog database if it doesn't exist
        self.init_catalog_db()?;
        
        // Get publisher names
        let publisher_names: Vec<String> = self.publishers.iter()
            .map(|p| p.name.clone())
            .collect();
        
        // Create the catalog and build it
        let catalog = ImageCatalog::new(self.catalog_dir(), self.catalog_db_path());
        catalog.build_catalog(&publisher_names).map_err(|e| {
            ImageError::Database(format!("Failed to build catalog: {}", e))
        })
    }
    
    /// Query the catalog for packages matching a pattern
    pub fn query_catalog(&self, pattern: Option<&str>) -> Result<Vec<PackageInfo>> {
        let catalog = ImageCatalog::new(self.catalog_dir(), self.catalog_db_path());
        catalog.query_packages(pattern).map_err(|e| {
            ImageError::Database(format!("Failed to query catalog: {}", e))
        })
    }
    
    /// Get a manifest from the catalog
    pub fn get_manifest_from_catalog(&self, fmri: &crate::fmri::Fmri) -> Result<Option<crate::actions::Manifest>> {
        let catalog = ImageCatalog::new(self.catalog_dir(), self.catalog_db_path());
        catalog.get_manifest(fmri).map_err(|e| {
            ImageError::Database(format!("Failed to get manifest from catalog: {}", e))
        })
    }
    
    /// Download catalog for a specific publisher
    pub fn download_publisher_catalog(&self, publisher_name: &str) -> Result<()> {
        // Get the publisher
        let publisher = self.get_publisher(publisher_name)?;
        
        // Create a REST backend for the publisher
        let mut repo = RestBackend::open(&publisher.origin)?;
        
        // Set local cache path to the catalog directory for this publisher
        let publisher_catalog_dir = self.catalog_dir().join(&publisher.name);
        fs::create_dir_all(&publisher_catalog_dir)?;
        repo.set_local_cache_path(&publisher_catalog_dir)?;
        
        // Download the catalog
        repo.download_catalog(&publisher.name, None)?;
        
        Ok(())
    }
    
    /// Create a new image with the basic directory structure
    /// 
    /// This method only creates the image structure without adding publishers or downloading catalogs.
    /// Publisher addition and catalog downloading should be handled separately.
    pub fn create_image<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Create a new image
        let image = Image::new_full(path.as_ref().to_path_buf());
        
        // Create the directory structure
        image.create_metadata_dir()?;
        image.create_manifest_dir()?;
        image.create_catalog_dir()?;
        
        // Initialize the installed packages database
        image.init_installed_db()?;
        
        // Initialize the catalog database
        image.init_catalog_db()?;
        
        // Save the image
        image.save()?;
        
        Ok(image)
    }

    /// Saves the image data to the metadata directory
    pub fn save(&self) -> Result<()> {
        self.create_metadata_dir()?;
        let json_path = self.image_json_path();
        let file = File::create(&json_path).map_err(|e| {
            ImageError::IO(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create image JSON file at {:?}: {}", json_path, e),
            ))
        })?;
        serde_json::to_writer_pretty(file, self).map_err(ImageError::Json)
    }

    /// Loads an image from the specified path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        // Check for both full and partial image JSON files
        let full_image = Image::new_full(path);
        let partial_image = Image::new_partial(path);
        
        let full_json_path = full_image.image_json_path();
        let partial_json_path = partial_image.image_json_path();
        
        // Determine which JSON file exists
        let json_path = if full_json_path.exists() {
            full_json_path
        } else if partial_json_path.exists() {
            partial_json_path
        } else {
            return Err(ImageError::InvalidPath(format!(
                "Image JSON file not found at either {:?} or {:?}", 
                full_json_path, partial_json_path
            )));
        };
        
        let file = File::open(&json_path).map_err(|e| {
            ImageError::IO(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open image JSON file at {:?}: {}", json_path, e),
            ))
        })?;
        
        serde_json::from_reader(file).map_err(ImageError::Json)
    }
}
