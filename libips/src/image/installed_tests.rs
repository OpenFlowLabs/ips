use super::*;
use crate::actions::{Attr, Manifest};
use crate::fmri::Fmri;
use redb::{Database, ReadableTable};
use std::str::FromStr;
use tempfile::tempdir;

#[test]
fn test_installed_packages() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().unwrap();
    let image_path = temp_dir.path().join("image");

    // Create the image
    let image = Image::create_image(&image_path, ImageType::Full).unwrap();

    // Verify that the installed packages database was initialized
    assert!(image.installed_db_path().exists());

    // Create a test manifest
    let mut manifest = Manifest::new();

    // Add some attributes to the manifest
    let mut attr = Attr::default();
    attr.key = "pkg.fmri".to_string();
    attr.values = vec!["pkg://test/example/package@1.0".to_string()];
    manifest.attributes.push(attr);

    let mut attr = Attr::default();
    attr.key = "pkg.summary".to_string();
    attr.values = vec!["Example package".to_string()];
    manifest.attributes.push(attr);

    let mut attr = Attr::default();
    attr.key = "pkg.description".to_string();
    attr.values = vec!["An example package for testing".to_string()];
    manifest.attributes.push(attr);

    // Create an FMRI for the package
    let fmri = Fmri::from_str("pkg://test/example/package@1.0").unwrap();

    // Install the package
    image.install_package(&fmri, &manifest).unwrap();

    // Verify that the package is installed
    assert!(image.is_package_installed(&fmri).unwrap());

    // Query the installed packages
    let packages = image.query_installed_packages(None).unwrap();

    // Verify that the package is in the results
    assert_eq!(packages.len(), 1);
    assert_eq!(
        packages[0].fmri.to_string(),
        "pkg://test/example/package@1.0"
    );
    assert_eq!(packages[0].publisher, "test");

    // Get the manifest from the installed packages database
    let installed_manifest = image.get_manifest_from_installed(&fmri).unwrap().unwrap();

    // Verify that the manifest is correct
    assert_eq!(installed_manifest.attributes.len(), 3);

    // Uninstall the package
    image.uninstall_package(&fmri).unwrap();

    // Verify that the package is no longer installed
    assert!(!image.is_package_installed(&fmri).unwrap());

    // Query the installed packages again
    let packages = image.query_installed_packages(None).unwrap();

    // Verify that there are no packages
    assert_eq!(packages.len(), 0);

    // Clean up
    temp_dir.close().unwrap();
}

#[test]
fn test_installed_packages_key_format() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("installed.redb");

    // Create the installed packages database
    let installed = InstalledPackages::new(&db_path);
    installed.init_db().unwrap();

    // Create a test manifest
    let mut manifest = Manifest::new();

    // Add some attributes to the manifest
    let mut attr = Attr::default();
    attr.key = "pkg.fmri".to_string();
    attr.values = vec!["pkg://test/example/package@1.0".to_string()];
    manifest.attributes.push(attr);

    // Create an FMRI for the package
    let fmri = Fmri::from_str("pkg://test/example/package@1.0").unwrap();

    // Add the package to the database
    installed.add_package(&fmri, &manifest).unwrap();

    // Open the database directly to check the key format
    let db = Database::open(&db_path).unwrap();
    let tx = db.begin_read().unwrap();
    let table = tx.open_table(installed::INSTALLED_TABLE).unwrap();

    // Iterate through the keys
    let mut keys = Vec::new();
    for entry in table.iter().unwrap() {
        let (key, _) = entry.unwrap();
        keys.push(key.value().to_string());
    }

    // Verify that there is one key and it has the correct format
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0], "pkg://test/example/package@1.0");

    // Clean up
    temp_dir.close().unwrap();
}
