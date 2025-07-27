use std::fs;
use std::path::Path;
use libips::repository::{FileBackend, WritableRepository, ReadableRepository, RepositoryVersion};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for the test
    let test_dir = Path::new("/tmp/pkg6_file_structure_test");
    if test_dir.exists() {
        fs::remove_dir_all(test_dir)?;
    }
    fs::create_dir_all(test_dir)?;
    
    println!("Created test directory: {}", test_dir.display());
    
    // Create a new repository
    let mut repo = FileBackend::create(test_dir, RepositoryVersion::V1)?;
    
    // Add a publisher
    repo.add_publisher("test")?;
    
    println!("Created repository with publisher 'test'");
    
    // Create a test file
    let test_file_path = test_dir.join("test_file.txt");
    fs::write(&test_file_path, "This is a test file")?;
    
    println!("Created test file: {}", test_file_path.display());
    
    // Store the file in the repository
    let hash = repo.store_file(&test_file_path)?;
    
    println!("Stored file with hash: {}", hash);
    
    // Check if the file was stored in the correct directory structure
    let first_two = &hash[0..2];
    let next_two = &hash[2..4];
    let expected_path = test_dir.join("file").join(first_two).join(next_two).join(&hash);
    
    if expected_path.exists() {
        println!("SUCCESS: File was stored at the correct path: {}", expected_path.display());
    } else {
        println!("ERROR: File was not stored at the expected path: {}", expected_path.display());
        
        // Check if the file was stored in the old location
        let old_path = test_dir.join("file").join(&hash);
        if old_path.exists() {
            println!("File was stored at the old path: {}", old_path.display());
        } else {
            println!("File was not stored at the old path either");
        }
    }
    
    // Clean up
    fs::remove_dir_all(test_dir)?;
    
    Ok(())
}