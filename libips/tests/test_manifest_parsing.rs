extern crate libips;

use libips::actions::Manifest;
use std::path::Path;

#[test]
fn test_parse_postgre_common_manifest() {
    let manifest_path = Path::new("/home/toasty/ws/illumos/ips/pkg6repo/postgre-common.manifest");
    let manifest = Manifest::parse_file(manifest_path).expect("Failed to parse manifest");

    // Check that the manifest contains the expected actions
    assert_eq!(manifest.attributes.len(), 11, "Expected 11 attributes");
    assert_eq!(manifest.directories.len(), 1, "Expected 1 directory");
    assert_eq!(manifest.groups.len(), 1, "Expected 1 group");
    assert_eq!(manifest.users.len(), 1, "Expected 1 user");
    assert_eq!(manifest.licenses.len(), 1, "Expected 1 license");

    // Check the group action
    let group = &manifest.groups[0];
    assert_eq!(
        group.groupname, "postgres",
        "Expected groupname to be 'postgres'"
    );
    assert_eq!(group.gid, "90", "Expected gid to be '90'");

    // Check the user action
    let user = &manifest.users[0];
    assert_eq!(
        user.username, "postgres",
        "Expected username to be 'postgres'"
    );
    assert_eq!(user.uid, "90", "Expected uid to be '90'");
    assert_eq!(user.group, "postgres", "Expected group to be 'postgres'");
    assert_eq!(
        user.home_dir, "/var/postgres",
        "Expected home_dir to be '/var/postgres'"
    );
    assert_eq!(
        user.login_shell, "/usr/bin/pfksh",
        "Expected login_shell to be '/usr/bin/pfksh'"
    );
    assert_eq!(user.password, "NP", "Expected password to be 'NP'");
    assert!(
        user.services.is_empty(),
        "Expected no services for ftpuser=false"
    );
    assert_eq!(
        user.gcos_field, "PostgreSQL Reserved UID",
        "Expected gcos_field to be 'PostgreSQL Reserved UID'"
    );

    // Check the directory action
    let dir = &manifest.directories[0];
    assert_eq!(
        dir.path, "var/postgres",
        "Expected path to be 'var/postgres'"
    );
    assert_eq!(dir.group, "postgres", "Expected group to be 'postgres'");
    assert_eq!(dir.owner, "postgres", "Expected owner to be 'postgres'");
    assert_eq!(dir.mode, "0755", "Expected mode to be '0755'");
}

#[test]
fn test_parse_pgadmin_manifest() {
    let manifest_path = Path::new("/home/toasty/ws/illumos/ips/pkg6repo/pgadmin.manifest");
    let manifest = Manifest::parse_file(manifest_path).expect("Failed to parse manifest");

    // Check that the manifest contains the expected actions
    assert!(manifest.attributes.len() > 0, "Expected attributes");
    assert!(manifest.files.len() > 0, "Expected files");
    assert_eq!(manifest.legacies.len(), 1, "Expected 1 legacy action");

    // Check the legacy action
    let legacy = &manifest.legacies[0];
    assert_eq!(legacy.arch, "i386", "Expected arch to be 'i386'");
    assert_eq!(
        legacy.category, "system",
        "Expected category to be 'system'"
    );
    assert_eq!(
        legacy.pkg, "SUNWpgadmin3",
        "Expected pkg to be 'SUNWpgadmin3'"
    );
    assert_eq!(
        legacy.vendor, "Project OpenIndiana",
        "Expected vendor to be 'Project OpenIndiana'"
    );
    assert_eq!(
        legacy.version, "11.11.0,REV=2010.05.25.01.00",
        "Expected version to be '11.11.0,REV=2010.05.25.01.00'"
    );
}
