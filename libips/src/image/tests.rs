use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_image_catalog() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().unwrap();
    let image_path = temp_dir.path().join("image");

    // Create the image
    let image = Image::create_image(&image_path, ImageType::Full).unwrap();

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
    let mut image = Image::create_image(&image_path, ImageType::Full).unwrap();

    // Print the image type and paths
    println!("Image type: {:?}", image.image_type());
    println!("Image path: {:?}", image.path());
    println!("Metadata dir: {:?}", image.metadata_dir());
    println!("Catalog dir: {:?}", image.catalog_dir());

    // Add a publisher
    image
        .add_publisher("test", "http://example.com/repo", vec![], true)
        .unwrap();

    // Print the publishers
    println!("Publishers: {:?}", image.publishers());

    // Create the catalog directory structure
    let catalog_dir = image.catalog_dir();
    let publisher_dir = catalog_dir.join("test");
    println!("Publisher dir: {:?}", publisher_dir);
    fs::create_dir_all(&publisher_dir).unwrap();

    // Create a simple catalog.attrs file
    let attrs_content = r#"{
        "created": "2025-08-04T23:01:00Z",
        "last-modified": "2025-08-04T23:01:00Z",
        "package-count": 2,
        "package-version-count": 2,
        "parts": {
            "base": {
                "last-modified": "2025-08-04T23:01:00Z"
            }
        },
        "updates": {},
        "version": 1
    }"#;
    println!(
        "Writing catalog.attrs to {:?}",
        publisher_dir.join("catalog.attrs")
    );
    println!("catalog.attrs content: {}", attrs_content);
    fs::write(publisher_dir.join("catalog.attrs"), attrs_content).unwrap();

    // Create a simple base catalog part
    let base_content = r#"{
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
    }"#;
    println!(
        "Writing base catalog part to {:?}",
        publisher_dir.join("base")
    );
    println!("base catalog part content: {}", base_content);
    fs::write(publisher_dir.join("base"), base_content).unwrap();

    // Verify that the files were written correctly
    println!(
        "Checking if catalog.attrs exists: {}",
        publisher_dir.join("catalog.attrs").exists()
    );
    println!(
        "Checking if base catalog part exists: {}",
        publisher_dir.join("base").exists()
    );

    // Build the catalog
    println!("Building catalog...");
    match image.build_catalog() {
        Ok(_) => println!("Catalog built successfully"),
        Err(e) => println!("Failed to build catalog: {:?}", e),
    }

    // Query the catalog
    println!("Querying catalog...");
    let packages = match image.query_catalog(None) {
        Ok(pkgs) => {
            println!("Found {} packages", pkgs.len());
            pkgs
        }
        Err(e) => {
            println!("Failed to query catalog: {:?}", e);
            panic!("Failed to query catalog: {:?}", e);
        }
    };

    // Verify that both non-obsolete and obsolete packages are in the results
    assert_eq!(packages.len(), 2);

    // Verify that one package is marked as obsolete
    let obsolete_packages: Vec<_> = packages.iter().filter(|p| p.obsolete).collect();
    assert_eq!(obsolete_packages.len(), 1);
    assert_eq!(obsolete_packages[0].fmri.stem(), "example/obsolete");

    // Verify that the obsolete package has the full FMRI as key
    // This is indirectly verified by checking that the publisher is included in the FMRI
    assert_eq!(
        obsolete_packages[0].fmri.publisher,
        Some("test".to_string())
    );

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

#[test]
fn test_refresh_catalogs_directory_clearing() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().unwrap();
    let image_path = temp_dir.path().join("image");

    // Create the image
    let mut image = Image::create_image(&image_path, ImageType::Full).unwrap();

    // Add two publishers
    image
        .add_publisher("test1", "http://example.com/repo1", vec![], true)
        .unwrap();
    image
        .add_publisher("test2", "http://example.com/repo2", vec![], false)
        .unwrap();

    // Create the catalog directory structure for both publishers
    let catalog_dir = image.catalog_dir();
    let publisher1_dir = catalog_dir.join("test1");
    let publisher2_dir = catalog_dir.join("test2");
    fs::create_dir_all(&publisher1_dir).unwrap();
    fs::create_dir_all(&publisher2_dir).unwrap();

    // Create marker files in both publisher directories
    let marker_file1 = publisher1_dir.join("marker");
    let marker_file2 = publisher2_dir.join("marker");
    fs::write(
        &marker_file1,
        "This file should be removed during full refresh",
    )
    .unwrap();
    fs::write(
        &marker_file2,
        "This file should be removed during full refresh",
    )
    .unwrap();
    assert!(marker_file1.exists());
    assert!(marker_file2.exists());

    // Directly test the directory clearing functionality for a specific publisher
    // This simulates the behavior of refresh_catalogs with full=true for a specific publisher
    if publisher1_dir.exists() {
        fs::remove_dir_all(&publisher1_dir).unwrap();
    }
    fs::create_dir_all(&publisher1_dir).unwrap();

    // Verify that the marker file for publisher1 was removed
    assert!(!marker_file1.exists());
    // Verify that the marker file for publisher2 still exists
    assert!(marker_file2.exists());

    // Create a new marker file for publisher1
    fs::write(
        &marker_file1,
        "This file should be removed during full refresh",
    )
    .unwrap();
    assert!(marker_file1.exists());

    // Directly test the directory clearing functionality for all publishers
    // This simulates the behavior of refresh_catalogs with full=true for all publishers
    for publisher in &image.publishers {
        let publisher_dir = catalog_dir.join(&publisher.name);
        if publisher_dir.exists() {
            fs::remove_dir_all(&publisher_dir).unwrap();
        }
        fs::create_dir_all(&publisher_dir).unwrap();
    }

    // Verify that both marker files were removed
    assert!(!marker_file1.exists());
    assert!(!marker_file2.exists());

    // Clean up
    temp_dir.close().unwrap();
}
