// End-to-end network test against OpenIndiana Hipster repository.
//
// This test is ignored by default to avoid network usage during CI runs.
// To run manually:
//   cargo test -p libips --test e2e_openindiana -- --ignored --nocapture
// Optionally set IPS_E2E_NET=1 to annotate that network is expected.
//
// What it does:
// - Creates a temporary Image (Full)
// - Adds publisher "openindiana.org" with origin https://pkg.openindiana.org/hipster
// - Downloads the publisher catalog via RestBackend
// - Builds the image-wide merged catalog
// - Asserts that we discover at least one package and can retrieve a manifest

use std::env;
use tempfile::tempdir;

use libips::image::{Image, ImageType};

fn should_run_network_tests() -> bool {
    // Even when ignored, provide an env switch to document intent
    env::var("IPS_E2E_NET").map(|v| v == "1" || v.to_lowercase() == "true").unwrap_or(false)
}

#[test]
#[ignore]
fn e2e_download_and_build_catalog_openindiana() {
    // If the env var is not set, just return early (test is ignored by default anyway)
    if !should_run_network_tests() {
        eprintln!(
            "Skipping e2e_download_and_build_catalog_openindiana (set IPS_E2E_NET=1 and run with --ignored to execute)"
        );
        return;
    }

    // Create a temporary directory for image
    let temp = tempdir().expect("failed to create temp dir");
    let img_path = temp.path().join("image");

    // Create the image
    let mut image = Image::create_image(&img_path, ImageType::Full).expect("failed to create image");

    // Add OpenIndiana publisher
    let publisher = "openindiana.org";
    let origin = "https://pkg.openindiana.org/hipster";
    image
        .add_publisher(publisher, origin, vec![], true)
        .expect("failed to add publisher");

    // Download catalog and build merged catalog
    image
        .download_publisher_catalog(publisher)
        .expect("failed to download publisher catalog");

    image.build_catalog().expect("failed to build merged catalog");

    // Query catalog; we expect at least one package
    let packages = image
        .query_catalog(None)
        .expect("failed to query catalog");

    assert!(
        !packages.is_empty(),
        "expected at least one package from OpenIndiana catalog"
    );

    // Attempt to get a manifest for the first package
    let some_pkg = &packages[0];
    let manifest_opt = image
        .get_manifest_from_catalog(&some_pkg.fmri)
        .expect("failed to get manifest from catalog");

    assert!(
        manifest_opt.is_some(),
        "expected to retrieve a manifest for at least one package"
    );

    // Optional debugging output
    eprintln!(
        "Fetched {} packages; example FMRI: {} (obsolete: {})",
        packages.len(),
        some_pkg.fmri,
        some_pkg.obsolete
    );
}
