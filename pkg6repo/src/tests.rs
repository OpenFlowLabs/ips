#[cfg(test)]
mod tests {
    use libips::repository::{Repository, RepositoryVersion, FileBackend, REPOSITORY_CONFIG_FILENAME};
    use tempfile::tempdir;

    #[test]
    fn test_create_repository() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        
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
    }
    
    #[test]
    fn test_add_publisher() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        
        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Add a publisher
        repo.add_publisher("example.com").unwrap();
        
        // Check that the publisher was added
        assert!(repo.config.publishers.contains(&"example.com".to_string()));
        assert!(repo_path.join("catalog").join("example.com").exists());
        assert!(repo_path.join("pkg").join("example.com").exists());
    }
    
    #[test]
    fn test_remove_publisher() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        
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
    }
    
    #[test]
    fn test_set_property() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path().join("repo");
        
        // Create a repository
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
        
        // Set a property
        repo.set_property("publisher/prefix", "example.com").unwrap();
        
        // Check that the property was set
        assert_eq!(repo.config.properties.get("publisher/prefix").unwrap(), "example.com");
    }
}