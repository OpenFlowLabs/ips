use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_image_catalog() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().unwrap();
    let image_path = temp_dir.path().join("image");
    
    // Create the image
    let image = Image::create_image(&image_path).unwrap();
    
    // Verify that the catalog database was initialized
    assert!(image.catalog_db_path().exists());
    
    // Clean up
    temp_dir.close().unwrap();
}

#[test]
fn test_catalog_methods() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().unwrap();
    let image_path = temp_dir.path().join("image");
    
    // Create the image
    let mut image = Image::create_image(&image_path).unwrap();
    
    // Add a publisher
    image.add_publisher("test", "http://example.com/repo", vec![], true).unwrap();
    
    // Create the catalog directory structure
    let catalog_dir = image.catalog_dir();
    let publisher_dir = catalog_dir.join("test");
    fs::create_dir_all(&publisher_dir).unwrap();
    
    // Create a simple catalog.attrs file
    let attrs_content = r#"{
        "parts": {
            "base": {}
        },
        "version": 1
    }"#;
    fs::write(publisher_dir.join("catalog.attrs"), attrs_content).unwrap();
    
    // Create a simple base catalog part
    let base_content = r#"{
        "packages": {
            "test": {
                "example/package": [
                    {
                        "version": "1.0",
                        "actions": [
                            "set name=pkg.fmri value=pkg://test/example/package@1.0",
                            "set name=pkg.summary value=\"Example package\"",
                            "set name=pkg.description value=\"An example package for testing\""
                        ]
                    }
                ],
                "example/obsolete": [
                    {
                        "version": "1.0",
                        "actions": [
                            "set name=pkg.fmri value=pkg://test/example/obsolete@1.0",
                            "set name=pkg.summary value=\"Obsolete package\"",
                            "set name=pkg.obsolete value=true"
                        ]
                    }
                ]
            }
        }
    }"#;
    fs::write(publisher_dir.join("base"), base_content).unwrap();
    
    // Build the catalog
    image.build_catalog().unwrap();
    
    // Query the catalog
    let packages = image.query_catalog(None).unwrap();
    
    // Verify that both non-obsolete and obsolete packages are in the results
    assert_eq!(packages.len(), 2);
    
    // Verify that one package is marked as obsolete
    let obsolete_packages: Vec<_> = packages.iter().filter(|p| p.obsolete).collect();
    assert_eq!(obsolete_packages.len(), 1);
    assert_eq!(obsolete_packages[0].fmri.stem(), "example/obsolete");
    
    // Verify that the obsolete package has the full FMRI as key
    // This is indirectly verified by checking that the publisher is included in the FMRI
    assert_eq!(obsolete_packages[0].fmri.publisher, Some("test".to_string()));
    
    // Verify that one package is not marked as obsolete
    let non_obsolete_packages: Vec<_> = packages.iter().filter(|p| !p.obsolete).collect();
    assert_eq!(non_obsolete_packages.len(), 1);
    assert_eq!(non_obsolete_packages[0].fmri.stem(), "example/package");
    
    // Get the manifest for the non-obsolete package
    let fmri = &non_obsolete_packages[0].fmri;
    let manifest = image.get_manifest_from_catalog(fmri).unwrap();
    assert!(manifest.is_some());
    
    // Get the manifest for the obsolete package
    let fmri = &obsolete_packages[0].fmri;
    let manifest = image.get_manifest_from_catalog(fmri).unwrap();
    assert!(manifest.is_some());
    
    // Verify that the obsolete package's manifest has the obsolete attribute
    let manifest = manifest.unwrap();
    let is_obsolete = manifest.attributes.iter().any(|attr| {
        attr.key == "pkg.obsolete" && attr.values.get(0).map_or(false, |v| v == "true")
    });
    assert!(is_obsolete);
    
    // Clean up
    temp_dir.close().unwrap();
}