mod properties;
#[cfg(test)]
mod tests;

use miette::Diagnostic;
use properties::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use thiserror::Error;

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
        }
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
