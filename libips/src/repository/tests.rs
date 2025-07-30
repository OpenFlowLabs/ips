//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(test)]
mod tests {
    use crate::actions::Manifest;
    use crate::fmri::Fmri;
    use crate::repository::{
        CatalogManager, FileBackend, ReadableRepository, RepositoryError, RepositoryVersion,
        Result, WritableRepository, REPOSITORY_CONFIG_FILENAME,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

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
        assert!(repo_path.join("file").exists());
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
        assert!(catalog_dir.join("test_part").exists());

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

        // Check that the files were published
        assert!(repo_path.join("file").exists());

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
}
