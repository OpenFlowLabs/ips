use crate::errors::DepotError;
use crate::repo::DepotRepo;
use axum::extract::{Path, Request, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::services::ServeFile;

/// Shard metadata entry in catalog.attrs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShardEntry {
    sha256: String,
    size: u64,
    #[serde(rename = "last-modified")]
    last_modified: String,
}

/// Shard index JSON structure for catalog/2/catalog.attrs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShardIndex {
    version: u32,
    created: String,
    #[serde(rename = "last-modified")]
    last_modified: String,
    #[serde(rename = "package-count")]
    package_count: usize,
    #[serde(rename = "package-version-count")]
    package_version_count: usize,
    shards: BTreeMap<String, ShardEntry>,
}

/// GET /{publisher}/catalog/2/catalog.attrs
pub async fn get_shard_index(
    State(repo): State<Arc<DepotRepo>>,
    Path(publisher): Path<String>,
) -> Result<Response, DepotError> {
    let shard_dir = repo.shard_dir(&publisher);
    let index_path = shard_dir.join("catalog.attrs");

    if !index_path.exists() {
        return Err(DepotError::Repo(
            libips::repository::RepositoryError::NotFound(
                "catalog.attrs not found - shards not yet built".to_string(),
            ),
        ));
    }

    let content = fs::read_to_string(&index_path)
        .map_err(|e| DepotError::Server(format!("Failed to read catalog.attrs: {}", e)))?;

    Ok(([(header::CONTENT_TYPE, "application/json")], content).into_response())
}

/// GET /{publisher}/catalog/2/{sha256}
pub async fn get_shard_blob(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, sha256)): Path<(String, String)>,
    req: Request,
) -> Result<Response, DepotError> {
    let shard_dir = repo.shard_dir(&publisher);
    let index_path = shard_dir.join("catalog.attrs");

    if !index_path.exists() {
        return Err(DepotError::Repo(
            libips::repository::RepositoryError::NotFound(
                "catalog.attrs not found - shards not yet built".to_string(),
            ),
        ));
    }

    // Read index to validate hash
    let index_content = fs::read_to_string(&index_path)
        .map_err(|e| DepotError::Server(format!("Failed to read catalog.attrs: {}", e)))?;
    let index: ShardIndex = serde_json::from_str(&index_content)
        .map_err(|e| DepotError::Server(format!("Failed to parse catalog.attrs: {}", e)))?;

    // Find which shard file corresponds to this hash
    let mut shard_path: Option<std::path::PathBuf> = None;
    for (name, entry) in &index.shards {
        if entry.sha256 == sha256 {
            shard_path = Some(shard_dir.join(&sha256));
            break;
        }
    }

    let Some(path) = shard_path else {
        return Err(DepotError::Repo(
            libips::repository::RepositoryError::NotFound(format!(
                "Shard with hash {} not found",
                sha256
            )),
        ));
    };

    if !path.exists() {
        return Err(DepotError::Repo(
            libips::repository::RepositoryError::NotFound(format!(
                "Shard file {} not found on disk",
                sha256
            )),
        ));
    }

    // Serve the file
    let service = ServeFile::new(path);
    let result = service.oneshot(req).await;

    match result {
        Ok(mut res) => {
            // Add cache headers - content is content-addressed and immutable
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/octet-stream"),
            );
            res.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, immutable, max-age=86400"),
            );
            Ok(res.into_response())
        }
        Err(e) => Err(DepotError::Server(e.to_string())),
    }
}
