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
    
    use libips::actions::Attr;
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
    
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);
    
    // 1. Test Versions
    let resp = client.get(format!("{}/versions/0/", base_url)).send().await.unwrap();
    assert!(resp.status().is_success());
    let text = resp.text().await.unwrap();
    assert!(text.contains("pkg-server pkg6depotd-0.5.1"));
    assert!(text.contains("catalog 1"));
    assert!(text.contains("manifest 0 1"));

    // 2. Test Catalog
    
    // Test Catalog v1
    let catalog_v1_url = format!("{}/test/catalog/1/catalog.attrs", base_url);
    let resp = client.get(&catalog_v1_url).send().await.unwrap();
    if !resp.status().is_success() {
         println!("Catalog v1 failed: {:?}", resp);
    }
    assert!(resp.status().is_success());
    let catalog_attrs = resp.text().await.unwrap();
    // Verify it looks like JSON catalog attrs (contains signature)
    assert!(catalog_attrs.contains("package-count"));
    assert!(catalog_attrs.contains("parts"));

    // 3. Test Manifest
    let fmri_arg = "example%401.0.0";
    // v0
    let manifest_url = format!("{}/test/manifest/0/{}", base_url, fmri_arg);
    let resp = client.get(&manifest_url).send().await.unwrap();
    assert!(resp.status().is_success());
    let manifest_text = resp.text().await.unwrap();
    assert!(manifest_text.contains("pkg.fmri"));
    assert!(manifest_text.contains("example@1.0.0"));
    
    // v1
    let manifest_v1_url = format!("{}/test/manifest/1/{}", base_url, fmri_arg);
    let resp = client.get(&manifest_v1_url).send().await.unwrap();
    assert!(resp.status().is_success());
    let manifest_text_v1 = resp.text().await.unwrap();
    assert_eq!(manifest_text, manifest_text_v1);

    // 4. Test Info
    let info_url = format!("{}/test/info/0/{}", base_url, fmri_arg);
    let resp = client.get(&info_url).send().await.unwrap();
    assert!(resp.status().is_success());
    let info_text = resp.text().await.unwrap();
    assert!(info_text.contains("Name: example"));
    assert!(info_text.contains("Summary: Test Package"));
    
    // 5. Test Publisher v1
    let pub_url = format!("{}/test/publisher/1", base_url);
    let resp = client.get(&pub_url).send().await.unwrap();
    assert!(resp.status().is_success());
    assert!(resp.headers().get("content-type").unwrap().to_str().unwrap().contains("application/vnd.pkg5.info"));
    let pub_json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(pub_json["version"], 1);
    assert_eq!(pub_json["publishers"][0]["name"], "test");
    
    // 6. Test File
    // We assume file exists if manifest works.
}

#[tokio::test]
async fn test_ini_only_repo_serving_catalog() {
    use libips::repository::{WritableRepository, ReadableRepository};
    use libips::repository::BatchOptions;
    use std::io::Write as _;

    // Setup temp repo
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo_ini");

    // Create repo using FileBackend to get layout, publish one pkg
    let mut backend = FileBackend::create(&repo_path, RepositoryVersion::V4).unwrap();
    let publisher = "ini-test";
    backend.add_publisher(publisher).unwrap();

    // Create content
    let content_dir = temp_dir.path().join("content_ini");
    fs::create_dir_all(&content_dir).unwrap();
    let file_path = content_dir.join("hello.txt");
    fs::write(&file_path, "Hello INI Repo").unwrap();

    // Publish one manifest
    let mut tx = backend.begin_transaction().unwrap();
    tx.set_publisher(publisher);
    let mut fa = FileAction::read_from_path(&file_path).unwrap();
    fa.path = "hello.txt".to_string();
    tx.add_file(fa, &file_path).unwrap();

    let mut manifest = Manifest::new();
    use libips::actions::Attr;
    use std::collections::HashMap;
    manifest.attributes.push(Attr { key: "pkg.fmri".to_string(), values: vec![format!("pkg://{}/example@1.0.0", publisher)], properties: HashMap::new() });
    manifest.attributes.push(Attr { key: "pkg.summary".to_string(), values: vec!["INI Repo Test Package".to_string()], properties: HashMap::new() });
    tx.update_manifest(manifest);
    tx.commit().unwrap();

    // Rebuild catalog using batched API explicitly with small batch to exercise code path
    let opts = BatchOptions { batch_size: 1, flush_every_n: 1 };
    backend.rebuild_catalog_batched(publisher, true, opts).unwrap();

    // Replace pkg6.repository with legacy pkg5.repository so FileBackend::open uses INI
    let pkg6_cfg = repo_path.join("pkg6.repository");
    if pkg6_cfg.exists() { fs::remove_file(&pkg6_cfg).unwrap(); }
    let mut ini = String::new();
    ini.push_str("[publisher]\n");
    ini.push_str(&format!("prefix = {}\n", publisher));
    ini.push_str("[repository]\nversion = 4\n");
    let mut f = std::fs::File::create(repo_path.join("pkg5.repository")).unwrap();
    f.write_all(ini.as_bytes()).unwrap();

    // Start depot server
    let config = Config {
        server: ServerConfig { bind: vec!["127.0.0.1:0".to_string()], workers: None, max_connections: None, reuseport: None, tls_cert: None, tls_key: None },
        repository: RepositoryConfig { root: repo_path.clone(), mode: Some("readonly".to_string()) },
        telemetry: None, publishers: None, admin: None, oauth2: None,
    };
    let repo = DepotRepo::new(&config).unwrap();
    let state = Arc::new(repo);
    let router = http::routes::app_router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { http::server::run(router, listener).await.unwrap(); });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // Fetch catalog attrs via v1 endpoint
    let url = format!("{}/{}/catalog/1/catalog.attrs", base_url, publisher);
    let resp = client.get(&url).send().await.unwrap();
    assert!(resp.status().is_success(), "status: {:?}", resp.status());
    let body = resp.text().await.unwrap();
    assert!(body.contains("package-count"));
    assert!(body.contains("parts"));
}
