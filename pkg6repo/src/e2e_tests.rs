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
        assert!(repo_path.join("publisher").exists());
        assert!(repo_path.join("index").exists());
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
        assert!(repo_path.join("publisher").join("example.com").exists());
        assert!(
            repo_path
                .join("publisher")
                .join("example.com")
                .join("catalog")
                .exists()
        );
        assert!(
            repo_path
                .join("publisher")
                .join("example.com")
                .join("pkg")
                .exists()
        );

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
    fn test_e2e_obsoleted_packages() {
        // Run the setup script to prepare the test environment
        let (prototype_dir, manifest_dir) = run_setup_script();

        // Create a test directory
        let test_dir = create_test_dir("e2e_obsoleted_packages");
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

        // Get the FMRI of the published package
        let result = run_pkg6repo(&["list", "-s", repo_path.to_str().unwrap(), "-F", "json"]);
        assert!(
            result.is_ok(),
            "Failed to list packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        let packages: serde_json::Value =
            serde_json::from_str(&output).expect("Failed to parse JSON output");

        // The FMRI in the JSON is an object with scheme, publisher, name, and version fields
        // We need to extract these fields and construct the FMRI string
        let fmri_obj = &packages["packages"][0]["fmri"];
        let scheme = fmri_obj["scheme"].as_str().expect("Failed to get scheme");
        let publisher = fmri_obj["publisher"]
            .as_str()
            .expect("Failed to get publisher");
        let name = fmri_obj["name"].as_str().expect("Failed to get name");
        let version_obj = &fmri_obj["version"];
        let release = version_obj["release"]
            .as_str()
            .expect("Failed to get release");

        // Construct the FMRI string in the format "pkg://publisher/name@version"
        let fmri = format!("{}://{}/{}", scheme, publisher, name);

        // Add version if available
        let fmri = if !release.is_empty() {
            format!("{}@{}", fmri, release)
        } else {
            fmri
        };

        // Print the FMRI and repo path for debugging
        println!("FMRI: {}", fmri);
        println!("Repo path: {}", repo_path.display());

        // Check if the package exists in the repository
        let pkg_dir = repo_path
            .join("publisher")
            .join("test")
            .join("pkg")
            .join("example");
        println!("Package directory: {}", pkg_dir.display());
        println!("Package directory exists: {}", pkg_dir.exists());

        // List files in the package directory if it exists
        if pkg_dir.exists() {
            println!("Files in package directory:");
            for entry in std::fs::read_dir(&pkg_dir).unwrap() {
                let entry = entry.unwrap();
                println!("  {}", entry.path().display());
            }
        }

        // Mark the package as obsoleted
        let result = run_pkg6repo(&[
            "obsolete-package",
            "-s",
            repo_path.to_str().unwrap(),
            "-p",
            "test",
            "-f",
            &fmri,
            "-m",
            "This package is obsoleted for testing purposes",
            "-r",
            "pkg://test/example2@1.0",
        ]);

        // Print the result for debugging
        println!("Result: {:?}", result);

        assert!(
            result.is_ok(),
            "Failed to mark package as obsoleted: {:?}",
            result.err()
        );

        // Verify the package is no longer in the main repository
        let result = run_pkg6repo(&["list", "-s", repo_path.to_str().unwrap()]);
        assert!(
            result.is_ok(),
            "Failed to list packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(
            !output.contains("example"),
            "Package still found in repository after being marked as obsoleted"
        );

        // List obsoleted packages
        let result = run_pkg6repo(&[
            "list-obsoleted",
            "-s",
            repo_path.to_str().unwrap(),
            "-p",
            "test",
        ]);
        assert!(
            result.is_ok(),
            "Failed to list obsoleted packages: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(
            output.contains("example"),
            "Obsoleted package not found in obsoleted packages list"
        );

        // Show details of the obsoleted package
        let result = run_pkg6repo(&[
            "show-obsoleted",
            "-s",
            repo_path.to_str().unwrap(),
            "-p",
            "test",
            "-f",
            &fmri,
        ]);
        assert!(
            result.is_ok(),
            "Failed to show obsoleted package details: {:?}",
            result.err()
        );

        let output = result.unwrap();
        assert!(
            output.contains("Status: obsolete"),
            "Package not marked as obsolete in details"
        );
        assert!(
            output.contains("This package is obsoleted for testing purposes"),
            "Deprecation message not found in details"
        );
        assert!(
            output.contains("pkg://test/example2@1.0"),
            "Replacement package not found in details"
        );

        // Clean up
        cleanup_test_dir(&test_dir);
    }
}
