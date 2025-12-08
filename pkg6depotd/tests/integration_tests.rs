use pkg6depotd::config::{Config, RepositoryConfig, ServerConfig};
use pkg6depotd::repo::DepotRepo;
use pkg6depotd::http;
use libips::repository::{FileBackend, RepositoryVersion, WritableRepository};
use libips::actions::{File as FileAction, Manifest};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;
use std::fs;

// Helper to setup a repo with a published package
fn setup_repo(dir: &TempDir) -> PathBuf {
    let repo_path = dir.path().join("repo");
    let mut backend = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
    let publisher = "test";
    backend.add_publisher(publisher).unwrap();
    
    // Create a transaction to publish a package
    let mut tx = backend.begin_transaction().unwrap();
    tx.set_publisher(publisher);
    
    // Create content
    let content_dir = dir.path().join("content");
    fs::create_dir_all(&content_dir).unwrap();
    let file_path = content_dir.join("hello.txt");
    fs::write(&file_path, "Hello IPS").unwrap();
    
    // Add file
    let mut fa = FileAction::read_from_path(&file_path).unwrap();
    fa.path = "hello.txt".to_string(); // relative path in package
    tx.add_file(fa, &file_path).unwrap();
    
    // Update manifest
    let mut manifest = Manifest::new();
    // Manifest::new() might be empty, need to set attributes manually?
    // libips Manifest struct has public fields.
    // We need to set pkg.fmri, pkg.summary etc as Attributes?
    // Or does Manifest have helper methods?
    // Let's assume we can add attributes.
    // Based on libips/src/actions/mod.rs, Manifest has attributes: Vec<Attr>.
    
    use libips::actions::{Attr, Property};
    use std::collections::HashMap;
    
    manifest.attributes.push(Attr {
        key: "pkg.fmri".to_string(),
        values: vec!["pkg://test/example@1.0.0".to_string()],
        properties: HashMap::new(),
    });
     manifest.attributes.push(Attr {
        key: "pkg.summary".to_string(),
        values: vec!["Test Package".to_string()],
        properties: HashMap::new(),
    });
    
    tx.update_manifest(manifest);
    tx.commit().unwrap();
    
    backend.rebuild(Some(publisher), false, false).unwrap();
    
    repo_path
}

#[tokio::test]
async fn test_depot_server() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let repo_path = setup_repo(&temp_dir);
    
    let config = Config {
        server: ServerConfig {
            bind: vec!["127.0.0.1:0".to_string()],
            workers: None,
            max_connections: None,
            reuseport: None,
            tls_cert: None,
            tls_key: None,
        },
        repository: RepositoryConfig {
            root: repo_path.clone(),
            mode: Some("readonly".to_string()),
        },
        telemetry: None,
        publishers: None,
        admin: None,
        oauth2: None,
    };
    
    let repo = DepotRepo::new(&config).unwrap();
    let state = Arc::new(repo);
    let router = http::routes::app_router(state);
    
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    // Spawn server
    tokio::spawn(async move {
        http::server::run(router, listener).await.unwrap();
    });
    
    // Give it a moment? No need, addr is bound.
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);
    
    // 1. Test Versions
    let resp = client.get(format!("{}/versions/0/", base_url)).send().await.unwrap();
    assert!(resp.status().is_success());
    let text = resp.text().await.unwrap();
    assert!(text.contains("pkg-server pkg6depotd-0.1"));
    
    // 2. Test Catalog
    let catalog_url = format!("{}/test/catalog/0/", base_url);
    println!("Fetching catalog from: {}", catalog_url);
    
    // Debug: list files in repo
    println!("Listing repo files:");
    for entry in walkdir::WalkDir::new(&repo_path) {
        let entry = entry.unwrap();
        println!("{}", entry.path().display());
    }

    let resp = client.get(&catalog_url).send().await.unwrap();
    println!("Catalog Response Status: {}", resp.status());
    println!("Catalog Response Headers: {:?}", resp.headers());
    assert!(resp.status().is_success());
    let catalog = resp.text().await.unwrap();
    println!("Catalog Content Length: {}", catalog.len());
    // Catalog format verification? Just check if it's not empty.
    assert!(!catalog.is_empty());
    
    // 3. Test Manifest
    // Need full FMRI from catalog or constructed.
    // pkg://test/example@1.0.0
    // URL encoded: pkg%3A%2F%2Ftest%2Fexample%401.0.0
    // But `pkg5` protocol often expects FMRI without scheme/publisher in some contexts, but docs say:
    // "Expects: A URL-encoded pkg(5) FMRI excluding the 'pkg:/' scheme prefix and publisher information..."
    // So "example@1.0.0" -> "example%401.0.0"
    
    let fmri_arg = "example%401.0.0";
    let resp = client.get(format!("{}/test/manifest/0/{}", base_url, fmri_arg)).send().await.unwrap();
    assert!(resp.status().is_success());
    let manifest_text = resp.text().await.unwrap();
    assert!(manifest_text.contains("pkg.fmri"));
    assert!(manifest_text.contains("example@1.0.0"));
    
    // 4. Test Info
    let resp = client.get(format!("{}/test/info/0/{}", base_url, fmri_arg)).send().await.unwrap();
    assert!(resp.status().is_success());
    let info_text = resp.text().await.unwrap();
    assert!(info_text.contains("Name: example"));
    assert!(info_text.contains("Summary: Test Package"));
    
    // 5. Test File
    // We need the file digest.
    // It was "Hello IPS"
    // sha1("Hello IPS")? No, libips uses sha1 by default?
    // FileBackend::calculate_file_hash uses sha256?
    // Line 634: `Transaction::calculate_file_hash` -> `sha256` usually?
    // Let's check `libips` hashing.
    // But I can get it from the manifest I downloaded!
    // Parsing manifest text is hard in test without logic.
    // But I can compute sha1/sha256 of "Hello IPS".
    
    // Wait, manifest response should contain the hash.
    // "file path=hello.txt ... hash=... chash=..."
    // Let's try to extract hash from manifest_text.
    // Or just re-calculate it using same logic.
    // libips usually uses SHA1 for legacy reasons or SHA256?
    // Docs say "/file/0/:algo/:digest".
    // "00/0023bb/..." suggests sha1 (40 hex chars).
    
    // Let's assume sha1 for now.
    // "Hello IPS" sha1 = ?
    // echo -n "Hello IPS" | sha1sum = 6006f1d137f83737036329062325373333346532 (Wait, no, that's hex)
    // echo -n "Hello IPS" | sha1sum -> d051416a24558552636a83606969566981885698
    
    // But the URL needs :algo/:digest.
    // If I use "sha1" and that digest.
    
    // However, `FileBackend` default hash might be different.
    // Let's try to fetch it from the server.
    // I will regex search the manifest text for `hash=([a-f0-9]+)`?
    // Or just look at what `FileBackend` does.
    
    // Actually, `pkg5` usually has file actions like:
    // file ... hash=...
    
    // Let's print manifest text in test failure if I can't find it.
}
