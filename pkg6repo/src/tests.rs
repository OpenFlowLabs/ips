#[cfg(test)]
mod tests {
    use libips::repository::{Repository, RepositoryVersion, FileBackend, REPOSITORY_CONFIG_FILENAME, PublisherInfo, RepositoryInfo};
    use std::path::PathBuf;
    use std::fs;

    // These tests interact with real repositories in a known location
    // instead of using temporary directories. This allows for better
    // debugging and inspection of the repositories during testing.
    
    // The base directory for all test repositories
    const TEST_REPO_BASE_DIR: &str = "/tmp/pkg6repo_test";
    
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

    #[test]
    fn test_create_repository() {
        // Create a real test directory
        let test_dir = create_test_dir("create_repository");
        let repo_path = test_dir.join("repo");
        
        // Create a repository
        let _ = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Check that the repository was created
        assert!(repo_path.exists());
        assert!(repo_path.join("catalog").exists());
        assert!(repo_path.join("file").exists());
        assert!(repo_path.join("index").exists());
        assert!(repo_path.join("pkg").exists());
        assert!(repo_path.join("trans").exists());
        assert!(repo_path.join(REPOSITORY_CONFIG_FILENAME).exists());
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_add_publisher() {
        // Create a real test directory
        let test_dir = create_test_dir("add_publisher");
        let repo_path = test_dir.join("repo");
        
        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Add a publisher
        repo.add_publisher("example.com").unwrap();
        
        // Check that the publisher was added
        assert!(repo.config.publishers.contains(&"example.com".to_string()));
        assert!(repo_path.join("catalog").join("example.com").exists());
        assert!(repo_path.join("pkg").join("example.com").exists());
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_remove_publisher() {
        // Create a real test directory
        let test_dir = create_test_dir("remove_publisher");
        let repo_path = test_dir.join("repo");
        
        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Add a publisher
        repo.add_publisher("example.com").unwrap();
        
        // Check that the publisher was added
        assert!(repo.config.publishers.contains(&"example.com".to_string()));
        
        // Remove the publisher
        repo.remove_publisher("example.com", false).unwrap();
        
        // Check that the publisher was removed
        assert!(!repo.config.publishers.contains(&"example.com".to_string()));
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_set_property() {
        // Create a real test directory
        let test_dir = create_test_dir("set_property");
        let repo_path = test_dir.join("repo");
        
        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Set a property
        repo.set_property("publisher/prefix", "example.com").unwrap();
        
        // Check that the property was set
        assert_eq!(repo.config.properties.get("publisher/prefix").unwrap(), "example.com");
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
    
    #[test]
    fn test_get_info() {
        // Create a real test directory
        let test_dir = create_test_dir("get_info");
        let repo_path = test_dir.join("repo");
        
        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Add a publisher
        repo.add_publisher("example.com").unwrap();
        
        // Get repository information
        let repo_info = repo.get_info().unwrap();
        
        // Check that the information is correct
        assert_eq!(repo_info.publishers.len(), 1);
        let publisher_info = &repo_info.publishers[0];
        assert_eq!(publisher_info.name, "example.com");
        assert_eq!(publisher_info.package_count, 0); // No packages yet
        assert_eq!(publisher_info.status, "online");
        
        // Clean up
        cleanup_test_dir(&test_dir);
    }
}