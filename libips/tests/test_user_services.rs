extern crate libips;

use libips::actions::Manifest;

#[test]
fn test_ftpuser_boolean_true() {
    // Create a manifest string with ftpuser=true
    let manifest_string = "user ftpuser=true".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check that "ftp" service is added
    assert!(
        user.services.contains("ftp"),
        "Expected 'ftp' service to be added when ftpuser=true"
    );
    assert_eq!(user.services.len(), 1, "Expected exactly one service");
}

#[test]
fn test_ftpuser_boolean_false() {
    // Create a manifest string with ftpuser=false
    let manifest_string = "user ftpuser=false".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check that no services are added
    assert!(
        user.services.is_empty(),
        "Expected no services when ftpuser=false"
    );
}

#[test]
fn test_ftpuser_services_list() {
    // Create a manifest string with ftpuser=ssh,ftp,http
    let manifest_string = "user ftpuser=ssh,ftp,http".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check that all services are added
    assert!(
        user.services.contains("ssh"),
        "Expected 'ssh' service to be added"
    );
    assert!(
        user.services.contains("ftp"),
        "Expected 'ftp' service to be added"
    );
    assert!(
        user.services.contains("http"),
        "Expected 'http' service to be added"
    );
    assert_eq!(user.services.len(), 3, "Expected exactly three services");
}

#[test]
fn test_ftpuser_services_with_whitespace() {
    // Create a manifest string with ftpuser=ssh, ftp, http
    let manifest_string = "user ftpuser=\"ssh, ftp, http\"".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check that all services are added with whitespace trimmed
    assert!(
        user.services.contains("ssh"),
        "Expected 'ssh' service to be added"
    );
    assert!(
        user.services.contains("ftp"),
        "Expected 'ftp' service to be added"
    );
    assert!(
        user.services.contains("http"),
        "Expected 'http' service to be added"
    );
    assert_eq!(user.services.len(), 3, "Expected exactly three services");
}

#[test]
fn test_ftpuser_empty_string() {
    // Create a manifest string with ftpuser=
    let manifest_string = "user ftpuser=".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check that no services are added
    assert!(
        user.services.is_empty(),
        "Expected no services for empty string"
    );
}

#[test]
fn test_ftpuser_with_empty_elements() {
    // Create a manifest string with ftpuser=ssh,,http
    let manifest_string = "user ftpuser=ssh,,http".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check that only non-empty services are added
    assert!(
        user.services.contains("ssh"),
        "Expected 'ssh' service to be added"
    );
    assert!(
        user.services.contains("http"),
        "Expected 'http' service to be added"
    );
    assert_eq!(user.services.len(), 2, "Expected exactly two services");
}

#[test]
fn test_real_world_example() {
    // Create a manifest string similar to the one in postgre-common.manifest
    let manifest_string = "user username=postgres uid=90 group=postgres home-dir=/var/postgres login-shell=/usr/bin/pfksh password=NP gcos-field=\"PostgreSQL Reserved UID\" ftpuser=false".to_string();

    // Parse the manifest
    let manifest = Manifest::parse_string(manifest_string).expect("Failed to parse manifest");

    // Get the user
    let user = &manifest.users[0];

    // Check user properties
    assert_eq!(user.username, "postgres");
    assert_eq!(user.uid, "90");
    assert_eq!(user.group, "postgres");
    assert_eq!(user.home_dir, "/var/postgres");
    assert_eq!(user.login_shell, "/usr/bin/pfksh");
    assert_eq!(user.password, "NP");
    assert_eq!(user.gcos_field, "PostgreSQL Reserved UID");
    assert!(
        user.services.is_empty(),
        "Expected no services for ftpuser=false"
    );
}
