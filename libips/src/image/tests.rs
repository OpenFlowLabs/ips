use super::*;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_image_types() {
    let full_image = Image::new_full("/");
    let partial_image = Image::new_partial("/tmp/partial");

    assert_eq!(*full_image.image_type(), ImageType::Full);
    assert_eq!(*partial_image.image_type(), ImageType::Partial);
}

#[test]
fn test_metadata_paths() {
    let full_image = Image::new_full("/");
    let partial_image = Image::new_partial("/tmp/partial");

    assert_eq!(full_image.metadata_dir(), Path::new("/var/pkg"));
    assert_eq!(partial_image.metadata_dir(), Path::new("/tmp/partial/.pkg"));

    assert_eq!(
        full_image.image_json_path(),
        Path::new("/var/pkg/pkg6.image.json")
    );
    assert_eq!(
        partial_image.image_json_path(),
        Path::new("/tmp/partial/.pkg/pkg6.image.json")
    );
}

#[test]
fn test_save_and_load() -> Result<()> {
    // Create a temporary directory for testing
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create a full image
    let mut full_image = Image::new_full(temp_path);
    
    // Add some test data
    full_image.props.push(ImageProperty::String("test_prop".to_string()));
    
    // Save the image
    full_image.save()?;
    
    // Check that the metadata directory and JSON file were created
    let metadata_dir = temp_path.join("var/pkg");
    let json_path = metadata_dir.join("pkg6.image.json");
    
    assert!(metadata_dir.exists());
    assert!(json_path.exists());
    
    // Load the image
    let loaded_image = Image::load(temp_path)?;
    
    // Check that the loaded image matches the original
    assert_eq!(*loaded_image.image_type(), ImageType::Full);
    assert_eq!(loaded_image.path, full_image.path);
    assert_eq!(loaded_image.props.len(), 1);
    
    // Clean up
    temp_dir.close().expect("Failed to clean up temp directory");
    
    Ok(())
}

#[test]
fn test_partial_image() -> Result<()> {
    // Create a temporary directory for testing
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    
    // Create a partial image
    let mut partial_image = Image::new_partial(temp_path);
    
    // Add some test data
    partial_image.props.push(ImageProperty::Boolean(true));
    
    // Save the image
    partial_image.save()?;
    
    // Check that the metadata directory and JSON file were created
    let metadata_dir = temp_path.join(".pkg");
    let json_path = metadata_dir.join("pkg6.image.json");
    
    assert!(metadata_dir.exists());
    assert!(json_path.exists());
    
    // Load the image
    let loaded_image = Image::load(temp_path)?;
    
    // Check that the loaded image matches the original
    assert_eq!(*loaded_image.image_type(), ImageType::Partial);
    assert_eq!(loaded_image.path, partial_image.path);
    assert_eq!(loaded_image.props.len(), 1);
    
    // Clean up
    temp_dir.close().expect("Failed to clean up temp directory");
    
    Ok(())
}

#[test]
fn test_invalid_path() {
    let result = Image::load("/nonexistent/path");
    assert!(result.is_err());
    
    if let Err(ImageError::InvalidPath(_)) = result {
        // Expected error
    } else {
        panic!("Expected InvalidPath error, got {:?}", result);
    }
}