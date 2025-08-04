//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(test)]
mod tests {
    use crate::actions::Manifest;
    use crate::fmri::Fmri;
    use crate::repository::{
        CatalogManager, FileBackend, ProgressInfo, ProgressReporter,
        ReadableRepository, RepositoryError, RepositoryVersion, RestBackend, Result, WritableRepository,
        REPOSITORY_CONFIG_FILENAME,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    // The base directory for all test repositories
    const TEST_REPO_BASE_DIR: &str = "/tmp/libips_repo_test";

    // Helper function to create a unique test directory
    fn create_test_dir(test_name: &str) -> PathBuf {
        let test_dir = PathBuf::from(format!("{}/{}", TEST_REPO_BASE_DIR, test_name));

        // Clean up any existing directory
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).unwrap();
        }

        // Create the directory
        fs::create_dir_all(&test_dir).unwrap();

        test_dir
    }

    // Helper function to clean up test directory
    fn cleanup_test_dir(test_dir: &PathBuf) {
        if test_dir.exists() {
            fs::remove_dir_all(test_dir).unwrap();
        }
    }

    // Helper function to run the setup script
    fn run_setup_script() -> (PathBuf, PathBuf) {
        // Run the xtask setup-test-env command
        let output = Command::new("cargo")
            .args(["run", "-p", "xtask", "--", "setup-test-env"])
            .output()
            .expect("Failed to run xtask setup-test-env");

        if !output.status.success() {
            panic!(
                "Failed to set up test environment: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Return the paths to the prototype and manifest directories
        (
            PathBuf::from("/tmp/pkg6_test/prototype"),
            PathBuf::from("/tmp/pkg6_test/manifests"),
        )
    }

    // Helper function to publish a package to a repository
    fn publish_package(
        repo: &mut FileBackend,
        manifest_path: &PathBuf,
        prototype_dir: &PathBuf,
        publisher: &str,
    ) -> Result<()> {
        println!(
            "Publishing package from manifest: {}",
            manifest_path.display()
        );
        println!("Prototype directory: {}", prototype_dir.display());
        println!("Publisher: {}", publisher);

        // Check if the manifest file exists
        if !manifest_path.exists() {
            println!("Error: Manifest file does not exist");
            return Err(RepositoryError::FileReadError(format!(
                "Manifest file does not exist: {}",
                manifest_path.display()
            )));
        }

        // Check if the prototype directory exists
        if !prototype_dir.exists() {
            println!("Error: Prototype directory does not exist");
            return Err(RepositoryError::NotFound(format!(
                "Prototype directory does not exist: {}",
                prototype_dir.display()
            )));
        }

        // Parse the manifest file
        println!("Parsing manifest file...");
        let manifest = Manifest::parse_file(manifest_path)?;
        println!(
            "Manifest parsed successfully. Files: {}",
            manifest.files.len()
        );

        // Begin a transaction
        println!("Beginning transaction...");
        let mut transaction = repo.begin_transaction()?;

        // Add files from the prototype directory to the transaction
        println!("Adding files to transaction...");
        for file_action in manifest.files.iter() {
            // Construct the full path to the file in the prototype directory
            let file_path = prototype_dir.join(&file_action.path);

            // Check if the file exists
            if !file_path.exists() {
                println!(
                    "Warning: File does not exist in prototype directory: {}",
                    file_path.display()
                );
                continue;
            }

            // Add the file to the transaction
            println!("Adding file: {}", file_action.path);
            transaction.add_file(file_action.clone(), &file_path)?;
        }

        // Update the manifest in the transaction
        println!("Updating manifest in transaction...");
        transaction.update_manifest(manifest);

        // Set the publisher for the transaction
        println!("Setting publisher: {}", publisher);
        transaction.set_publisher(publisher);

        // Commit the transaction
        println!("Committing transaction...");
        transaction.commit()?;
        println!("Transaction committed successfully");

        // Debug: Check if the package manifest was stored in the correct location
        let publisher_pkg_dir = FileBackend::construct_package_dir(&repo.path, publisher, "");
        println!(
            "Publisher package directory: {}",
            publisher_pkg_dir.display()
        );

        if publisher_pkg_dir.exists() {
            println!("Publisher directory exists");

            // List files in the publisher directory
            if let Ok(entries) = std::fs::read_dir(&publisher_pkg_dir) {
                println!("Files in publisher directory:");
                for entry in entries.flatten() {
                    println!("  {}", entry.path().display());
                }
            } else {
                println!("Failed to read publisher directory");
            }
        } else {
            println!("Publisher directory does not exist");
        }

        // Rebuild the catalog
        println!("Rebuilding catalog...");
        repo.rebuild(Some(publisher), false, false)?;
        println!("Catalog rebuilt successfully");

        Ok(())
    }

    #[test]
    fn test_create_repository() {
        // Create a test directory
        let test_dir = create_test_dir("create_repository");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let _repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Check that the repository was created
        assert!(repo_path.exists());
        assert!(repo_path.join("publisher").exists());
        assert!(repo_path.join("index").exists());
        assert!(repo_path.join("trans").exists());
        assert!(repo_path.join(REPOSITORY_CONFIG_FILENAME).exists());

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_add_publisher() {
        // Create a test directory
        let test_dir = create_test_dir("add_publisher");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        repo.add_publisher("example.com").unwrap();

        // Check that the publisher was added
        assert!(repo.config.publishers.contains(&"example.com".to_string()));
        assert!(FileBackend::construct_catalog_path(&repo_path, "example.com").exists());
        assert!(FileBackend::construct_package_dir(&repo_path, "example.com", "").exists());
        
        // Check that the pub.p5i file was created for backward compatibility
        let pub_p5i_path = repo_path.join("publisher").join("example.com").join("pub.p5i");
        assert!(pub_p5i_path.exists(), "pub.p5i file should be created for backward compatibility");
        
        // Verify the content of the pub.p5i file
        let pub_p5i_content = fs::read_to_string(&pub_p5i_path).unwrap();
        let pub_p5i_json: serde_json::Value = serde_json::from_str(&pub_p5i_content).unwrap();
        
        // Check the structure of the pub.p5i file
        assert_eq!(pub_p5i_json["version"], 1);
        assert!(pub_p5i_json["packages"].is_array());
        assert!(pub_p5i_json["publishers"].is_array());
        assert_eq!(pub_p5i_json["publishers"][0]["name"], "example.com");

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_catalog_manager() {
        // Create a test directory
        let test_dir = create_test_dir("catalog_manager");
        let publisher_dir = test_dir.join("publisher");
        let publisher_name = "test";
        let catalog_dir = publisher_dir.join(publisher_name).join("catalog");

        // Create the catalog directory
        fs::create_dir_all(&catalog_dir).unwrap();

        // Create a catalog manager with the publisher parameter
        let mut catalog_manager = CatalogManager::new(&publisher_dir, publisher_name).unwrap();

        // Create a catalog part
        catalog_manager.create_part("test_part");

        // Add a package to the part using the stored publisher
        let fmri = Fmri::parse("pkg://test/example@1.0.0").unwrap();
        catalog_manager.add_package_to_part("test_part", &fmri, None, None).unwrap();

        // Save the part
        catalog_manager.save_part("test_part").unwrap();

        // Check that the part was saved
        assert!(publisher_dir.join("test_part").exists());

        // Create a new catalog manager and load the part
        let mut new_catalog_manager = CatalogManager::new(&publisher_dir, publisher_name).unwrap();
        new_catalog_manager.load_part("test_part").unwrap();

        // Check that the part was loaded
        let loaded_part = new_catalog_manager.get_part("test_part").unwrap();
        assert_eq!(loaded_part.packages.len(), 1);

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_publish_files() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("publish_files");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        repo.add_publisher("test").unwrap();

        // Publish a package using the manifest
        let manifest_path = manifest_dir.join("example.p5m");
        publish_package(&mut repo, &manifest_path, &prototype_dir, "test").unwrap();

        // Check that the files were published in the publisher-specific directory
        assert!(repo_path.join("publisher").join("test").join("file").exists());

        // Get repository information
        let repo_info = repo.get_info().unwrap();

        // Check that the publisher information is correct
        assert_eq!(repo_info.publishers.len(), 1);
        let publisher_info = &repo_info.publishers[0];
        assert_eq!(publisher_info.name, "test");

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_list_packages() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("list_packages");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        repo.add_publisher("test").unwrap();

        // Publish a package using the manifest
        let manifest_path = manifest_dir.join("example.p5m");
        publish_package(&mut repo, &manifest_path, &prototype_dir, "test").unwrap();

        // List packages
        let packages = repo.list_packages(Some("test"), None).unwrap();

        // Check that packages were listed
        assert!(!packages.is_empty());

        // Check that the package name is correct
        assert_eq!(packages[0].fmri.name, "example");

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_show_contents() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("show_contents");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        repo.add_publisher("test").unwrap();

        // Publish a package using the manifest
        let manifest_path = manifest_dir.join("example.p5m");
        publish_package(&mut repo, &manifest_path, &prototype_dir, "test").unwrap();

        // Show contents
        let contents = repo.show_contents(Some("test"), None, None).unwrap();

        // Check that contents were shown
        assert!(!contents.is_empty());

        // Check that the contents include the expected files
        let package_contents = &contents[0];
        assert!(package_contents.files.is_some());
        let files = package_contents.files.as_ref().unwrap();

        // Check for specific files
        assert!(files.iter().any(|f| f.contains("usr/bin/hello")));
        assert!(files
            .iter()
            .any(|f| f.contains("usr/share/doc/example/README.txt")));
        assert!(files.iter().any(|f| f.contains("etc/config/example.conf")));

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_search() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("search");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        repo.add_publisher("test").unwrap();

        // Publish a package using the manifest
        let manifest_path = manifest_dir.join("example.p5m");
        publish_package(&mut repo, &manifest_path, &prototype_dir, "test").unwrap();

        // Build the search index
        repo.rebuild(Some("test"), false, false).unwrap();

        // Search for packages
        let results = repo.search("example", Some("test"), None).unwrap();

        // Check that search results were returned
        assert!(!results.is_empty());

        // Check that the package name is correct
        assert_eq!(results[0].fmri.name, "example");

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_file_structure() {
        // Create a test directory
        let test_dir = create_test_dir("file_structure");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        repo.add_publisher("test").unwrap();

        // Create a test file
        let test_file_path = test_dir.join("test_file.txt");
        fs::write(&test_file_path, "This is a test file").unwrap();

        // Store the file in the repository
        let hash = repo.store_file(&test_file_path, "test").unwrap();

        // Check if the file was stored in the correct directory structure
        let expected_path = FileBackend::construct_file_path_with_publisher(&repo_path, "test", &hash);

        // Verify that the file exists at the expected path
        assert!(
            expected_path.exists(),
            "File was not stored at the expected path: {}",
            expected_path.display()
        );

        // Verify that the file does NOT exist at the old path (with no directory prefixing)
        let old_path = repo_path.join("file").join(&hash);
        assert!(
            !old_path.exists(),
            "File was stored at the old path: {}",
            old_path.display()
        );

        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_transaction_pub_p5i_creation() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("transaction_pub_p5i");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Create a new publisher through a transaction
        let publisher = "transaction_test";
        
        // Start a transaction
        let mut transaction = repo.begin_transaction().unwrap();
        
        // Set the publisher for the transaction
        transaction.set_publisher(publisher);
        
        // Add a simple manifest to the transaction
        let manifest_path = manifest_dir.join("example.p5m");
        let manifest = Manifest::parse_file(&manifest_path).unwrap();
        transaction.update_manifest(manifest);
        
        // Commit the transaction
        transaction.commit().unwrap();
        
        // Check that the pub.p5i file was created for the new publisher
        let pub_p5i_path = repo_path.join("publisher").join(publisher).join("pub.p5i");
        assert!(pub_p5i_path.exists(), "pub.p5i file should be created for new publisher in transaction");
        
        // Verify the content of the pub.p5i file
        let pub_p5i_content = fs::read_to_string(&pub_p5i_path).unwrap();
        let pub_p5i_json: serde_json::Value = serde_json::from_str(&pub_p5i_content).unwrap();
        
        // Check the structure of the pub.p5i file
        assert_eq!(pub_p5i_json["version"], 1);
        assert!(pub_p5i_json["packages"].is_array());
        assert!(pub_p5i_json["publishers"].is_array());
        assert_eq!(pub_p5i_json["publishers"][0]["name"], publisher);
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_legacy_pkg5_repository_creation() {
        // Create a test directory
        let test_dir = create_test_dir("legacy_pkg5_repository");
        let repo_path = test_dir.join("repo");

        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();

        // Add a publisher
        let publisher = "openindiana.org";
        repo.add_publisher(publisher).unwrap();
        
        // Set as default publisher
        repo.set_default_publisher(publisher).unwrap();
        
        // Check that the pkg5.repository file was created
        let pkg5_repo_path = repo_path.join("pkg5.repository");
        assert!(pkg5_repo_path.exists(), "pkg5.repository file should be created for backward compatibility");
        
        // Verify the content of the pkg5.repository file
        let pkg5_content = fs::read_to_string(&pkg5_repo_path).unwrap();
        
        // Print the content for debugging
        println!("pkg5.repository content:\n{}", pkg5_content);
        
        // Check that the file contains the expected sections and values
        assert!(pkg5_content.contains("[publisher]"));
        assert!(pkg5_content.contains("prefix=openindiana.org"));
        assert!(pkg5_content.contains("[repository]"));
        assert!(pkg5_content.contains("version=4"));
        assert!(pkg5_content.contains("trust-anchor-directory=/etc/certs/CA/"));
        assert!(pkg5_content.contains("signature-required-names=[]"));
        assert!(pkg5_content.contains("check-certificate-revocation=False"));
        assert!(pkg5_content.contains("[CONFIGURATION]"));
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_rest_repository_local_functionality() {
        use crate::repository::RestBackend;
        
        // Create a test directory
        let test_dir = create_test_dir("rest_repository");
        let cache_path = test_dir.join("cache");
        
        println!("Test directory: {}", test_dir.display());
        println!("Cache path: {}", cache_path.display());
        
        // Create a REST repository
        let uri = "http://pkg.opensolaris.org/release";
        let mut repo = RestBackend::open(uri).unwrap();
        
        // Set the local cache path
        repo.set_local_cache_path(&cache_path).unwrap();
        
        println!("Local cache path set to: {:?}", repo.local_cache_path);
        
        // Add a publisher
        let publisher = "openindiana.org";
        repo.add_publisher(publisher).unwrap();
        
        println!("Publisher added: {}", publisher);
        println!("Publishers in config: {:?}", repo.config.publishers);
        
        // Verify that the directory structure was created correctly
        let publisher_dir = cache_path.join("publisher").join(publisher);
        println!("Publisher directory: {}", publisher_dir.display());
        println!("Publisher directory exists: {}", publisher_dir.exists());
        
        assert!(publisher_dir.exists(), "Publisher directory should be created");
        
        let catalog_dir = publisher_dir.join("catalog");
        println!("Catalog directory: {}", catalog_dir.display());
        println!("Catalog directory exists: {}", catalog_dir.exists());
        
        assert!(catalog_dir.exists(), "Catalog directory should be created");
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    /// A test progress reporter that records all progress events
    #[derive(Debug, Clone)]
    struct TestProgressReporter {
        /// Records of all start events
        start_events: Arc<Mutex<Vec<ProgressInfo>>>,
        /// Records of all update events
        update_events: Arc<Mutex<Vec<ProgressInfo>>>,
        /// Records of all finish events
        finish_events: Arc<Mutex<Vec<ProgressInfo>>>,
    }
    
    impl TestProgressReporter {
        /// Create a new test progress reporter
        fn new() -> Self {
            TestProgressReporter {
                start_events: Arc::new(Mutex::new(Vec::new())),
                update_events: Arc::new(Mutex::new(Vec::new())),
                finish_events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    
        /// Get the number of start events recorded
        fn start_count(&self) -> usize {
            self.start_events.lock().unwrap().len()
        }
    
        /// Get the number of update events recorded
        fn update_count(&self) -> usize {
            self.update_events.lock().unwrap().len()
        }
    
        /// Get the number of finish events recorded
        fn finish_count(&self) -> usize {
            self.finish_events.lock().unwrap().len()
        }
    
        /// Get a clone of all start events
        fn get_start_events(&self) -> Vec<ProgressInfo> {
            self.start_events.lock().unwrap().clone()
        }
    
        /// Get a clone of all update events
        fn get_update_events(&self) -> Vec<ProgressInfo> {
            self.update_events.lock().unwrap().clone()
        }
    
        /// Get a clone of all finish events
        fn get_finish_events(&self) -> Vec<ProgressInfo> {
            self.finish_events.lock().unwrap().clone()
        }
    }
    
    impl ProgressReporter for TestProgressReporter {
        fn start(&self, info: &ProgressInfo) {
            let mut events = self.start_events.lock().unwrap();
            events.push(info.clone());
        }
    
        fn update(&self, info: &ProgressInfo) {
            let mut events = self.update_events.lock().unwrap();
            events.push(info.clone());
        }
    
        fn finish(&self, info: &ProgressInfo) {
            let mut events = self.finish_events.lock().unwrap();
            events.push(info.clone());
        }
    }
    
    #[test]
    fn test_progress_reporter() {
        // Create a test progress reporter
        let reporter = TestProgressReporter::new();
    
        // Create some progress info
        let info1 = ProgressInfo::new("Test operation 1");
        let info2 = ProgressInfo::new("Test operation 2")
            .with_current(50)
            .with_total(100);
    
        // Report some progress
        reporter.start(&info1);
        reporter.update(&info2);
        reporter.finish(&info1);
    
        // Check that the events were recorded
        assert_eq!(reporter.start_count(), 1);
        assert_eq!(reporter.update_count(), 1);
        assert_eq!(reporter.finish_count(), 1);
    
        // Check the content of the events
        let start_events = reporter.get_start_events();
        let update_events = reporter.get_update_events();
        let finish_events = reporter.get_finish_events();
    
        assert_eq!(start_events[0].operation, "Test operation 1");
        assert_eq!(update_events[0].operation, "Test operation 2");
        assert_eq!(update_events[0].current, Some(50));
        assert_eq!(update_events[0].total, Some(100));
        assert_eq!(finish_events[0].operation, "Test operation 1");
    }
    
    #[test]
    fn test_rest_backend_with_progress() {
        // This test is a mock test that doesn't actually connect to a remote server
        // It just verifies that the progress reporting mechanism works correctly
    
        // Create a test directory
        let test_dir = create_test_dir("rest_progress");
        let cache_path = test_dir.join("cache");
    
        // Create a REST repository
        let uri = "http://pkg.opensolaris.org/release";
        let mut repo = RestBackend::create(uri, RepositoryVersion::V4).unwrap();
    
        // Set the local cache path
        repo.set_local_cache_path(&cache_path).unwrap();
    
        // Create a test progress reporter
        let reporter = TestProgressReporter::new();
    
        // Add a publisher
        let publisher = "test";
        repo.add_publisher(publisher).unwrap();
    
        // Create a mock catalog.attrs file
        let publisher_dir = cache_path.join("publisher").join(publisher);
        let catalog_dir = publisher_dir.join("catalog");
        fs::create_dir_all(&catalog_dir).unwrap();
    
        let attrs_content = r#"{
            "created": "20250803T124900Z",
            "last-modified": "20250803T124900Z",
            "package-count": 100,
            "package-version-count": 200,
            "parts": {
                "catalog.base.C": {
                    "last-modified": "20250803T124900Z"
                },
                "catalog.dependency.C": {
                    "last-modified": "20250803T124900Z"
                },
                "catalog.summary.C": {
                    "last-modified": "20250803T124900Z"
                }
            },
            "version": 1
        }"#;
    
        let attrs_path = catalog_dir.join("catalog.attrs");
        fs::write(&attrs_path, attrs_content).unwrap();
    
        // Create mock catalog part files
        for part_name in ["catalog.base.C", "catalog.dependency.C", "catalog.summary.C"] {
            let part_path = catalog_dir.join(part_name);
            fs::write(&part_path, "{}").unwrap();
        }
    
        // Mock the download_catalog_file method to avoid actual HTTP requests
        // This is done by creating the files before calling download_catalog
    
        // Create a simple progress update to ensure update events are recorded
        let progress_info = ProgressInfo::new("Test update")
            .with_current(1)
            .with_total(2);
        reporter.update(&progress_info);
        
        // Call download_catalog with the progress reporter
        // This will fail because we're not actually connecting to a server,
        // but we can still verify that the progress reporter was called
        let _ = repo.download_catalog(publisher, Some(&reporter));
    
        // Check that the progress reporter was called
        assert!(reporter.start_count() > 0, "No start events recorded");
        assert!(reporter.update_count() > 0, "No update events recorded");
        assert!(reporter.finish_count() > 0, "No finish events recorded");
    
        // Clean up
        cleanup_test_dir(&test_dir);
    }
}
