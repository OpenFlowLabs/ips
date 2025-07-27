//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(test)]
mod e2e_tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::str;

    // The base directory for all test repositories
    const TEST_REPO_BASE_DIR: &str = "/tmp/pkg6repo_e2e_test";

    // Get the path to the pre-built binaries
    fn get_bin_dir() -> PathBuf {
        match env::var("PKG6_TEST_BIN_DIR") {
            Ok(dir) => PathBuf::from(dir),
            Err(_) => {
                // Fallback to the default location if the environment variable is not set
                PathBuf::from("/tmp/pkg6_test/bin")
            }
        }
    }

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

    // Helper function to set up the test environment
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

    // Helper function to run pkg6repo command
    fn run_pkg6repo(args: &[&str]) -> Result<String, String> {
        let bin_dir = get_bin_dir();
        let pkg6repo_bin = bin_dir.join("pkg6repo");

        // Check if the binary exists
        if !pkg6repo_bin.exists() {
            return Err(format!(
                "pkg6repo binary not found at {}. Run 'cargo xtask build-e2e' first.",
                pkg6repo_bin.display()
            ));
        }

        let output = Command::new(pkg6repo_bin)
            .args(args)
            .output()
            .expect("Failed to execute pkg6repo command");

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    // Helper function to run pkg6dev command
    fn run_pkg6dev(args: &[&str]) -> Result<String, String> {
        let bin_dir = get_bin_dir();
        let pkg6dev_bin = bin_dir.join("pkg6dev");

        // Check if the binary exists
        if !pkg6dev_bin.exists() {
            return Err(format!(
                "pkg6dev binary not found at {}. Run 'cargo xtask build-e2e' first.",
                pkg6dev_bin.display()
            ));
        }

        let output = Command::new(pkg6dev_bin)
            .args(args)
            .output()
            .expect("Failed to execute pkg6dev command");

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    #[test]
    fn test_e2e_create_repository() {
        // Create a test directory
        let test_dir = create_test_dir("e2e_create_repository");
        let repo_path = test_dir.join("repo");

        // Create a repository using pkg6repo
        let result = run_pkg6repo(&["create", "--repo-version", "4", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to create repository: {:?}",
            result.err()
        );

        // Check that the repository was created
        assert!(repo_path.exists());
        assert!(repo_path.join("catalog").exists());
        assert!(repo_path.join("file").exists());
        assert!(repo_path.join("index").exists());
        assert!(repo_path.join("pkg").exists());
        assert!(repo_path.join("trans").exists());
        assert!(repo_path.join("pkg6.repository").exists());

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_e2e_add_publisher() {
        // Create a test directory
        let test_dir = create_test_dir("e2e_add_publisher");
        let repo_path = test_dir.join("repo");

        // Create a repository using pkg6repo
        let result = run_pkg6repo(&["create", "--repo-version", "4", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to create repository: {:?}",
            result.err()
        );

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&[
            "add-publisher",
            "-s",
            repo_path.to_str().unwrap(),
            "example.com",
        ]);
        assert!(
            result.is_ok(),
            "Failed to add publisher: {:?}",
            result.err()
        );

        // Check that the publisher was added
        assert!(repo_path.join("catalog").join("example.com").exists());
        assert!(repo_path.join("pkg").join("example.com").exists());

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_e2e_publish_package() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("e2e_publish_package");
        let repo_path = test_dir.join("repo");

        // Create a repository using pkg6repo
        let result = run_pkg6repo(&["create", "--repo-version", "4", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to create repository: {:?}",
            result.err()
        );

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&["add-publisher", "-s", repo_path.to_str().unwrap(), "test"]);
        assert!(
            result.is_ok(),
            "Failed to add publisher: {:?}",
            result.err()
        );

        // Publish a package using pkg6dev
        let manifest_path = manifest_dir.join("example.p5m");
        let result = run_pkg6dev(&[
            "publish",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "--prototype-dir",
            prototype_dir.to_str().unwrap(),
            "--repo-path",
            repo_path.to_str().unwrap(),
        ]);
        assert!(
            result.is_ok(),
            "Failed to publish package: {:?}",
            result.err()
        );

        // Check that the package was published
        let result = run_pkg6repo(&["list", "-s", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to list packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(
            output.contains("example"),
            "Package not found in repository"
        );

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_e2e_show_contents() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("e2e_show_contents");
        let repo_path = test_dir.join("repo");

        // Create a repository using pkg6repo
        let result = run_pkg6repo(&["create", "--repo-version", "4", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to create repository: {:?}",
            result.err()
        );

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&["add-publisher", "-s", repo_path.to_str().unwrap(), "test"]);
        assert!(
            result.is_ok(),
            "Failed to add publisher: {:?}",
            result.err()
        );

        // Publish a package using pkg6dev
        let manifest_path = manifest_dir.join("example.p5m");
        let result = run_pkg6dev(&[
            "publish",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "--prototype-dir",
            prototype_dir.to_str().unwrap(),
            "--repo-path",
            repo_path.to_str().unwrap(),
        ]);
        assert!(
            result.is_ok(),
            "Failed to publish package: {:?}",
            result.err()
        );

        // Show package contents using pkg6repo
        let result = run_pkg6repo(&["contents", "-s", repo_path.to_str().unwrap(), "example"]);
        assert!(
            result.is_ok(),
            "Failed to show package contents: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(
            output.contains("usr/bin/hello"),
            "File not found in package contents"
        );
        assert!(
            output.contains("usr/share/doc/example/README.txt"),
            "File not found in package contents"
        );
        assert!(
            output.contains("etc/config/example.conf"),
            "File not found in package contents"
        );

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_e2e_multiple_packages() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("e2e_multiple_packages");
        let repo_path = test_dir.join("repo");

        // Create a repository using pkg6repo
        let result = run_pkg6repo(&["create", "--repo-version", "4", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to create repository: {:?}",
            result.err()
        );

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&["add-publisher", "-s", repo_path.to_str().unwrap(), "test"]);
        assert!(
            result.is_ok(),
            "Failed to add publisher: {:?}",
            result.err()
        );

        // Publish the first package using pkg6dev
        let manifest_path1 = manifest_dir.join("example.p5m");
        let result = run_pkg6dev(&[
            "publish",
            "--manifest-path",
            manifest_path1.to_str().unwrap(),
            "--prototype-dir",
            prototype_dir.to_str().unwrap(),
            "--repo-path",
            repo_path.to_str().unwrap(),
        ]);
        assert!(
            result.is_ok(),
            "Failed to publish first package: {:?}",
            result.err()
        );

        // Publish the second package using pkg6dev
        let manifest_path2 = manifest_dir.join("example2.p5m");
        let result = run_pkg6dev(&[
            "publish",
            "--manifest-path",
            manifest_path2.to_str().unwrap(),
            "--prototype-dir",
            prototype_dir.to_str().unwrap(),
            "--repo-path",
            repo_path.to_str().unwrap(),
        ]);
        assert!(
            result.is_ok(),
            "Failed to publish second package: {:?}",
            result.err()
        );

        // List packages using pkg6repo
        let result = run_pkg6repo(&["list", "-s", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to list packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(
            output.contains("example"),
            "First package not found in repository"
        );
        assert!(
            output.contains("example2"),
            "Second package not found in repository"
        );

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_e2e_import_pkg5_directory() {
        // Get the path to the sample pkg5 repository
        let sample_repo_path = PathBuf::from(env::current_dir().unwrap())
            .join("sample_data")
            .join("sample-repo");

        // Check if the sample repository exists
        if !sample_repo_path.exists() {
            println!(
                "Sample pkg5 repository not found at {}, skipping test",
                sample_repo_path.display()
            );
            return;
        }

        // Create a test directory
        let test_dir = create_test_dir("e2e_import_pkg5_directory");
        let repo_path = test_dir.join("repo");

        // Import the pkg5 repository using pkg6repo
        let result = run_pkg6repo(&[
            "import-pkg5",
            "--source",
            sample_repo_path.to_str().unwrap(),
            "--destination",
            repo_path.to_str().unwrap(),
        ]);
        assert!(
            result.is_ok(),
            "Failed to import pkg5 repository: {:?}",
            result.err()
        );

        // Check that the repository was created
        assert!(repo_path.exists());
        assert!(repo_path.join("catalog").exists());
        assert!(repo_path.join("file").exists());
        assert!(repo_path.join("index").exists());
        assert!(repo_path.join("pkg").exists());
        assert!(repo_path.join("pkg6.repository").exists());

        // Check that the publisher was imported
        assert!(repo_path.join("pkg").join("openindiana.org").exists());

        // List packages using pkg6repo
        let result = run_pkg6repo(&["list", "-s", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to list packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(!output.is_empty(), "No packages found in repository");

        // Clean up
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_e2e_import_pkg5_archive() {
        // Get the path to the sample pkg5 p5p archive
        let sample_p5p_path = PathBuf::from(env::current_dir().unwrap())
            .join("sample_data")
            .join("sample-repo.p5p");

        // Check if the sample p5p archive exists
        if !sample_p5p_path.exists() {
            println!(
                "Sample pkg5 p5p archive not found at {}, skipping test",
                sample_p5p_path.display()
            );
            return;
        }

        // Create a test directory
        let test_dir = create_test_dir("e2e_import_pkg5_archive");
        let repo_path = test_dir.join("repo");

        // Import the pkg5 p5p archive using pkg6repo
        let result = run_pkg6repo(&[
            "import-pkg5",
            "--source",
            sample_p5p_path.to_str().unwrap(),
            "--destination",
            repo_path.to_str().unwrap(),
        ]);
        assert!(
            result.is_ok(),
            "Failed to import pkg5 p5p archive: {:?}",
            result.err()
        );

        // Check that the repository was created
        assert!(repo_path.exists());
        assert!(repo_path.join("catalog").exists());
        assert!(repo_path.join("file").exists());
        assert!(repo_path.join("index").exists());
        assert!(repo_path.join("pkg").exists());
        assert!(repo_path.join("pkg6.repository").exists());

        // Check that the publisher was imported
        assert!(repo_path.join("pkg").join("openindiana.org").exists());

        // List packages using pkg6repo
        let result = run_pkg6repo(&["list", "-s", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to list packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(!output.is_empty(), "No packages found in repository");

        // Clean up
        cleanup_test_dir(&test_dir);
    }
}
