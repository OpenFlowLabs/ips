use crate::error::{Pkg6RepoError, Result};
use libips::actions::Manifest;
use libips::repository::{FileBackend, ReadableRepository, WritableRepository};
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use tracing::{debug, info, warn};

/// Represents a pkg5 repository importer
pub struct Pkg5Importer {
    /// Path to the pkg5 repository (directory or p5p archive)
    source_path: PathBuf,
    /// Path to the destination repository
    dest_path: PathBuf,
    /// Whether the source is a p5p archive
    is_p5p: bool,
    /// Temporary directory for extraction (if a source is a p5p archive)
    temp_dir: Option<tempfile::TempDir>,
}

impl Pkg5Importer {
    /// Creates a new Pkg5Importer
    pub fn new<P: AsRef<Path>>(source_path: P, dest_path: P) -> Result<Self> {
        let source_path = source_path.as_ref().to_path_buf();
        let dest_path = dest_path.as_ref().to_path_buf();

        debug!(
            "Creating Pkg5Importer with source: {}, destination: {}",
            source_path.display(),
            dest_path.display()
        );

        // Check if a source path exists
        if !source_path.exists() {
            debug!("Source path does not exist: {}", source_path.display());
            return Err(Pkg6RepoError::from(format!(
                "Source path does not exist: {}",
                source_path.display()
            )));
        }
        debug!("Source path exists: {}", source_path.display());

        // Determine if a source is a p5p archive
        let is_p5p =
            source_path.is_file() && source_path.extension().map_or(false, |ext| ext == "p5p");
        debug!("Source is p5p archive: {}", is_p5p);

        Ok(Self {
            source_path,
            dest_path,
            is_p5p,
            temp_dir: None,
        })
    }

    /// Prepares the source repository for import
    fn prepare_source(&mut self) -> Result<PathBuf> {
        if self.is_p5p {
            // Create a temporary directory for extraction
            let temp_dir = tempdir().map_err(|e| {
                Pkg6RepoError::from(format!("Failed to create temporary directory: {}", e))
            })?;

            info!(
                "Extracting p5p archive to temporary directory: {}",
                temp_dir.path().display()
            );

            // Extract the p5p archive to the temporary directory
            let status = std::process::Command::new("tar")
                .arg("-xf")
                .arg(&self.source_path)
                .arg("-C")
                .arg(temp_dir.path())
                .status()
                .map_err(|e| {
                    Pkg6RepoError::from(format!("Failed to extract p5p archive: {}", e))
                })?;

            if !status.success() {
                return Err(Pkg6RepoError::from(format!(
                    "Failed to extract p5p archive: {}",
                    status
                )));
            }

            // Store the temporary directory
            let source_path = temp_dir.path().to_path_buf();
            self.temp_dir = Some(temp_dir);

            Ok(source_path)
        } else {
            // Source is already a directory
            Ok(self.source_path.clone())
        }
    }

    /// Imports the pkg5 repository
    pub fn import(&mut self, publisher: Option<&str>) -> Result<()> {
        debug!("Starting import with publisher: {:?}", publisher);

        // Prepare the source repository
        debug!("Preparing source repository");
        let source_path = self.prepare_source()?;
        debug!("Source repository prepared: {}", source_path.display());

        // Check if this is a pkg5 repository
        let pkg5_repo_file = source_path.join("pkg5.repository");
        let pkg5_index_file = source_path.join("pkg5.index.0.gz");

        debug!(
            "Checking if pkg5.repository exists: {}",
            pkg5_repo_file.exists()
        );
        debug!(
            "Checking if pkg5.index.0.gz exists: {}",
            pkg5_index_file.exists()
        );

        if !pkg5_repo_file.exists() && !pkg5_index_file.exists() {
            debug!(
                "Source does not appear to be a pkg5 repository: {}",
                source_path.display()
            );
            return Err(Pkg6RepoError::from(format!(
                "Source does not appear to be a pkg5 repository: {}",
                source_path.display()
            )));
        }

        // Open or create the destination repository
        debug!(
            "Checking if destination repository exists: {}",
            self.dest_path.exists()
        );
        let mut dest_repo = if self.dest_path.exists() {
            // Check if it's a valid repository by looking for the pkg6.repository file
            let repo_config_file = self.dest_path.join("pkg6.repository");
            debug!(
                "Checking if repository config file exists: {}",
                repo_config_file.exists()
            );

            if repo_config_file.exists() {
                // It's a valid repository, open it
                info!("Opening existing repository: {}", self.dest_path.display());
                debug!(
                    "Attempting to open repository at: {}",
                    self.dest_path.display()
                );
                FileBackend::open(&self.dest_path)?
            } else {
                // It's not a valid repository, create a new one
                info!(
                    "Destination exists but is not a valid repository, creating a new one: {}",
                    self.dest_path.display()
                );
                debug!(
                    "Attempting to create repository at: {}",
                    self.dest_path.display()
                );
                FileBackend::create(&self.dest_path, libips::repository::RepositoryVersion::V4)?
            }
        } else {
            // Destination doesn't exist, create a new repository
            info!("Creating new repository: {}", self.dest_path.display());
            debug!(
                "Attempting to create repository at: {}",
                self.dest_path.display()
            );
            FileBackend::create(&self.dest_path, libips::repository::RepositoryVersion::V4)?
        };

        // Find publishers in the source repository
        let publishers = self.find_publishers(&source_path)?;

        if publishers.is_empty() {
            return Err(Pkg6RepoError::from(
                "No publishers found in source repository".to_string(),
            ));
        }

        // Determine which publisher to import
        let publisher_to_import = match publisher {
            Some(pub_name) => {
                if !publishers.iter().any(|p| p == pub_name) {
                    return Err(Pkg6RepoError::from(format!(
                        "Publisher not found in source repository: {}",
                        pub_name
                    )));
                }
                pub_name
            }
            None => {
                // Use the first publisher if none specified
                &publishers[0]
            }
        };

        info!("Importing from publisher: {}", publisher_to_import);

        // Ensure the publisher exists in the destination repository
        if !dest_repo
            .config
            .publishers
            .iter()
            .any(|p| p == publisher_to_import)
        {
            info!(
                "Adding publisher to destination repository: {}",
                publisher_to_import
            );
            dest_repo.add_publisher(publisher_to_import)?;

            // Set as the default publisher if there isn't one already
            if dest_repo.config.default_publisher.is_none() {
                info!("Setting as default publisher: {}", publisher_to_import);
                dest_repo.set_default_publisher(publisher_to_import)?;
            }
        }

        // Import packages
        self.import_packages(&source_path, &mut dest_repo, publisher_to_import)?;

        // Rebuild catalog and search index
        info!("Rebuilding catalog and search index...");
        dest_repo.rebuild(Some(publisher_to_import), false, false)?;

        info!("Import completed successfully");
        Ok(())
    }

    /// Finds publishers in the source repository
    fn find_publishers(&self, source_path: &Path) -> Result<Vec<String>> {
        let publisher_dir = source_path.join("publisher");

        if !publisher_dir.exists() || !publisher_dir.is_dir() {
            return Err(Pkg6RepoError::from(format!(
                "Publisher directory not found: {}",
                publisher_dir.display()
            )));
        }

        let mut publishers = Vec::new();

        for entry in fs::read_dir(&publisher_dir).map_err(|e| Pkg6RepoError::IoError(e))? {
            let entry = entry.map_err(|e| Pkg6RepoError::IoError(e))?;

            let path = entry.path();

            if path.is_dir() {
                let publisher = path.file_name().unwrap().to_string_lossy().to_string();
                publishers.push(publisher);
            }
        }

        Ok(publishers)
    }

    /// Imports packages from the source repository
    fn import_packages(
        &self,
        source_path: &Path,
        dest_repo: &mut FileBackend,
        publisher: &str,
    ) -> Result<()> {
        let pkg_dir = source_path.join("publisher").join(publisher).join("pkg");

        if !pkg_dir.exists() || !pkg_dir.is_dir() {
            return Err(Pkg6RepoError::from(format!(
                "Package directory not found: {}",
                pkg_dir.display()
            )));
        }

        // Create a temporary directory for extracted files
        let temp_proto_dir = tempdir().map_err(|e| {
            Pkg6RepoError::from(format!(
                "Failed to create temporary prototype directory: {}",
                e
            ))
        })?;

        info!(
            "Created temporary prototype directory: {}",
            temp_proto_dir.path().display()
        );

        // Find package directories
        let mut package_count = 0;

        for pkg_entry in fs::read_dir(&pkg_dir).map_err(|e| Pkg6RepoError::IoError(e))? {
            let pkg_entry = pkg_entry.map_err(|e| Pkg6RepoError::IoError(e))?;

            let pkg_path = pkg_entry.path();

            if pkg_path.is_dir() {
                // This is a package directory
                let pkg_name = pkg_path.file_name().unwrap().to_string_lossy().to_string();
                let decoded_pkg_name = url_decode(&pkg_name);

                debug!("Processing package: {}", decoded_pkg_name);

                // Find package versions
                for ver_entry in fs::read_dir(&pkg_path).map_err(|e| Pkg6RepoError::IoError(e))? {
                    let ver_entry = ver_entry.map_err(|e| Pkg6RepoError::IoError(e))?;

                    let ver_path = ver_entry.path();

                    if ver_path.is_file() {
                        // This is a package version
                        let ver_name = ver_path.file_name().unwrap().to_string_lossy().to_string();
                        let decoded_ver_name = url_decode(&ver_name);

                        debug!("Processing version: {}", decoded_ver_name);

                        // Import this package version
                        self.import_package_version(
                            source_path,
                            dest_repo,
                            publisher,
                            &ver_path,
                            &decoded_pkg_name,
                            &decoded_ver_name,
                            temp_proto_dir.path(),
                        )?;

                        package_count += 1;
                    }
                }
            }
        }

        info!("Imported {} packages", package_count);
        Ok(())
    }

    /// Imports a specific package version
    fn import_package_version(
        &self,
        source_path: &Path,
        dest_repo: &mut FileBackend,
        publisher: &str,
        manifest_path: &Path,
        pkg_name: &str,
        _ver_name: &str,
        proto_dir: &Path,
    ) -> Result<()> {
        debug!("Importing package version from {}", manifest_path.display());

        // Extract package name from FMRI
        debug!("Extracted package name from FMRI: {}", pkg_name);

        // Read the manifest file content
        debug!(
            "Reading manifest file content from {}",
            manifest_path.display()
        );
        let manifest_content = fs::read_to_string(manifest_path).map_err(|e| {
            debug!("Error reading manifest file: {}", e);
            Pkg6RepoError::IoError(e)
        })?;

        // Parse the manifest using parse_string
        debug!("Parsing manifest content");
        let manifest = Manifest::parse_string(manifest_content)?;

        // Begin a transaction
        debug!("Beginning transaction");
        let mut transaction = dest_repo.begin_transaction()?;

        // Set the publisher for the transaction
        debug!("Using specified publisher: {}", publisher);
        transaction.set_publisher(publisher);

        // Debug the repository structure
        debug!(
            "Publisher directory: {}",
            dest_repo.path.join("pkg").join(publisher).display()
        );

        // Extract files referenced in the manifest
        let file_dir = source_path.join("publisher").join(publisher).join("file");

        if !file_dir.exists() || !file_dir.is_dir() {
            return Err(Pkg6RepoError::from(format!(
                "File directory not found: {}",
                file_dir.display()
            )));
        }

        // Process file actions
        for file_action in manifest.files.iter() {
            // Extract the hash from the file action's payload
            if let Some(payload) = &file_action.payload {
                let hash = payload.primary_identifier.hash.clone();

                // Determine the file path in the source repository
                // Try the new two-level hierarchy first (first two characters, then next two characters)
                let first_two = &hash[0..2];
                let next_two = &hash[2..4];
                let file_path_new = file_dir.join(first_two).join(next_two).join(&hash);
                
                // Fall back to the old one-level hierarchy if the file doesn't exist in the new structure
                let file_path_old = file_dir.join(first_two).join(&hash);
                
                // Use the path that exists
                let file_path = if file_path_new.exists() {
                    file_path_new
                } else {
                    file_path_old
                };

                if !file_path.exists() {
                    warn!(
                        "File not found in source repository: {}",
                        file_path.display()
                    );
                    continue;
                }

                // Extract the file to the prototype directory
                let proto_file_path = proto_dir.join(&file_action.path);

                // Create parent directories if they don't exist
                if let Some(parent) = proto_file_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| Pkg6RepoError::IoError(e))?;
                }

                // Extract the gzipped file
                let mut source_file =
                    File::open(&file_path).map_err(|e| Pkg6RepoError::IoError(e))?;

                let mut dest_file =
                    File::create(&proto_file_path).map_err(|e| Pkg6RepoError::IoError(e))?;

                // Check if the file is gzipped
                let mut header = [0; 2];
                source_file
                    .read_exact(&mut header)
                    .map_err(|e| Pkg6RepoError::IoError(e))?;

                // Reset file position
                source_file
                    .seek(std::io::SeekFrom::Start(0))
                    .map_err(|e| Pkg6RepoError::IoError(e))?;

                if header[0] == 0x1f && header[1] == 0x8b {
                    // File is gzipped, decompress it
                    let mut decoder = flate2::read::GzDecoder::new(source_file);
                    std::io::copy(&mut decoder, &mut dest_file)
                        .map_err(|e| Pkg6RepoError::IoError(e))?;
                } else {
                    // File is not gzipped, copy it as is
                    std::io::copy(&mut source_file, &mut dest_file)
                        .map_err(|e| Pkg6RepoError::IoError(e))?;
                }

                // Add the file to the transaction
                transaction.add_file(file_action.clone(), &proto_file_path)?;
            }
        }

        // Update the manifest in the transaction
        transaction.update_manifest(manifest);

        // The Transaction.commit() method will handle creating necessary directories
        // and storing the manifest in the correct location, so we don't need to create
        // package-specific directories here.

        // Commit the transaction
        transaction.commit()?;

        Ok(())
    }
}

/// URL decodes a string
fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i < s.len() {
        if s[i..].starts_with("%") && i + 2 < s.len() {
            if let Ok(hex) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(hex as char);
                i += 3;
            } else {
                result.push('%');
                i += 1;
            }
        } else {
            result.push(s[i..].chars().next().unwrap());
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("test"), "test");
        assert_eq!(url_decode("test%20test"), "test test");
        assert_eq!(url_decode("test%2Ftest"), "test/test");
        assert_eq!(url_decode("test%2Ctest"), "test,test");
        assert_eq!(url_decode("test%3Atest"), "test:test");
    }
}
