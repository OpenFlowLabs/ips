//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(test)]
mod e2e_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::str;

    // The base directory for all test repositories
    const TEST_REPO_BASE_DIR: &str = "/tmp/pkg6repo_e2e_test";

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
        // Get the project root directory
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .expect("Failed to execute git command");

        let project_root = String::from_utf8(output.stdout)
            .expect("Invalid UTF-8 output")
            .trim()
            .to_string();

        // Run the setup script
        Command::new("bash")
            .arg(format!("{}/setup_test_env.sh", project_root))
            .status()
            .expect("Failed to run setup script");

        // Return the paths to the prototype and manifest directories
        (
            PathBuf::from("/tmp/pkg6_test/prototype"),
            PathBuf::from("/tmp/pkg6_test/manifests"),
        )
    }

    // Helper function to run pkg6repo command
    fn run_pkg6repo(args: &[&str]) -> Result<String, String> {
        let output = Command::new("cargo")
            .arg("run")
            .arg("--bin")
            .arg("pkg6repo")
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
        let output = Command::new("cargo")
            .arg("run")
            .arg("--bin")
            .arg("pkg6dev")
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
        assert!(result.is_ok(), "Failed to create repository: {:?}", result.err());

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
        assert!(result.is_ok(), "Failed to create repository: {:?}", result.err());

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&[
            "add-publisher",
            repo_path.to_str().unwrap(),
            "example.com",
        ]);
        assert!(result.is_ok(), "Failed to add publisher: {:?}", result.err());

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
        assert!(result.is_ok(), "Failed to create repository: {:?}", result.err());

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&[
            "add-publisher",
            repo_path.to_str().unwrap(),
            "test",
        ]);
        assert!(result.is_ok(), "Failed to add publisher: {:?}", result.err());

        // Publish a package using pkg6dev
        let manifest_path = manifest_dir.join("example.p5m");
        let result = run_pkg6dev(&[
            "publish",
            manifest_path.to_str().unwrap(),
            prototype_dir.to_str().unwrap(),
            repo_path.to_str().unwrap(),
        ]);
        assert!(result.is_ok(), "Failed to publish package: {:?}", result.err());

        // Check that the package was published
        let result = run_pkg6repo(&[
            "list",
            repo_path.to_str().unwrap(),
        ]);
        assert!(result.is_ok(), "Failed to list packages: {:?}", result.err());
        
        let output = result.unwrap();
        assert!(output.contains("example"), "Package not found in repository");

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
        assert!(result.is_ok(), "Failed to create repository: {:?}", result.err());

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&[
            "add-publisher",
            repo_path.to_str().unwrap(),
            "test",
        ]);
        assert!(result.is_ok(), "Failed to add publisher: {:?}", result.err());

        // Publish a package using pkg6dev
        let manifest_path = manifest_dir.join("example.p5m");
        let result = run_pkg6dev(&[
            "publish",
            manifest_path.to_str().unwrap(),
            prototype_dir.to_str().unwrap(),
            repo_path.to_str().unwrap(),
        ]);
        assert!(result.is_ok(), "Failed to publish package: {:?}", result.err());

        // Show package contents using pkg6repo
        let result = run_pkg6repo(&[
            "contents",
            repo_path.to_str().unwrap(),
            "example",
        ]);
        assert!(result.is_ok(), "Failed to show package contents: {:?}", result.err());
        
        let output = result.unwrap();
        assert!(output.contains("usr/bin/hello"), "File not found in package contents");
        assert!(output.contains("usr/share/doc/example/README.txt"), "File not found in package contents");
        assert!(output.contains("etc/config/example.conf"), "File not found in package contents");

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
        assert!(result.is_ok(), "Failed to create repository: {:?}", result.err());

        // Add a publisher using pkg6repo
        let result = run_pkg6repo(&[
            "add-publisher",
            repo_path.to_str().unwrap(),
            "test",
        ]);
        assert!(result.is_ok(), "Failed to add publisher: {:?}", result.err());

        // Publish the first package using pkg6dev
        let manifest_path1 = manifest_dir.join("example.p5m");
        let result = run_pkg6dev(&[
            "publish",
            manifest_path1.to_str().unwrap(),
            prototype_dir.to_str().unwrap(),
            repo_path.to_str().unwrap(),
        ]);
        assert!(result.is_ok(), "Failed to publish first package: {:?}", result.err());

        // Publish the second package using pkg6dev
        let manifest_path2 = manifest_dir.join("example2.p5m");
        let result = run_pkg6dev(&[
            "publish",
            manifest_path2.to_str().unwrap(),
            prototype_dir.to_str().unwrap(),
            repo_path.to_str().unwrap(),
        ]);
        assert!(result.is_ok(), "Failed to publish second package: {:?}", result.err());

        // List packages using pkg6repo
        let result = run_pkg6repo(&[
            "list",
            repo_path.to_str().unwrap(),
        ]);
        assert!(result.is_ok(), "Failed to list packages: {:?}", result.err());
        
        let output = result.unwrap();
        assert!(output.contains("example"), "First package not found in repository");
        assert!(output.contains("example2"), "Second package not found in repository");

        // Clean up
        cleanup_test_dir(&test_dir);
    }
}