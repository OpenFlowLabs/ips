//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

//! Client-side shard synchronization.
//!
//! Downloads catalog shards from the repository server and verifies their integrity.

use crate::repository::sqlite_catalog::ShardIndex;
use miette::Diagnostic;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("Shard sync error: {message}")]
#[diagnostic(code(ips::shard_sync_error))]
pub struct ShardSyncError {
    pub message: String,
}

impl ShardSyncError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl From<reqwest::Error> for ShardSyncError {
    fn from(e: reqwest::Error) -> Self {
        Self::new(format!("HTTP error: {}", e))
    }
}

impl From<std::io::Error> for ShardSyncError {
    fn from(e: std::io::Error) -> Self {
        Self::new(format!("IO error: {}", e))
    }
}

impl From<serde_json::Error> for ShardSyncError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(format!("JSON error: {}", e))
    }
}

/// Synchronize catalog shards from a repository origin.
///
/// Downloads the shard index from `{origin_url}/{publisher}/catalog/2/catalog.attrs`,
/// compares hashes with local copies, and downloads only changed shards.
///
/// # Arguments
/// * `publisher` - Publisher name
/// * `origin_url` - Repository origin URL (e.g., "https://pkg.example.com")
/// * `local_shard_dir` - Local directory to store shards
/// * `download_obsolete` - Whether to download obsolete.db (default: false)
pub fn sync_shards(
    publisher: &str,
    origin_url: &str,
    local_shard_dir: &Path,
    download_obsolete: bool,
) -> Result<(), ShardSyncError> {
    // Ensure local directory exists
    fs::create_dir_all(local_shard_dir)?;

    // Fetch shard index
    let index_url = format!("{}/{}/catalog/2/catalog.attrs", origin_url, publisher);
    let client = reqwest::blocking::Client::new();
    let response = client.get(&index_url).send()?;

    if !response.status().is_success() {
        return Err(ShardSyncError::new(format!(
            "Failed to fetch shard index: HTTP {}",
            response.status()
        )));
    }

    let index: ShardIndex = response.json()?;

    // List of shards to sync
    let shards_to_sync = if download_obsolete {
        vec!["active.db", "fts.db", "obsolete.db"]
    } else {
        vec!["active.db", "fts.db"]
    };

    // Download each shard if needed
    for shard_name in shards_to_sync {
        let Some(shard_entry) = index.shards.get(shard_name) else {
            tracing::warn!("Shard {} not found in index", shard_name);
            continue;
        };

        let local_path = local_shard_dir.join(shard_name);

        // Check if local copy exists and matches hash
        let needs_download = if local_path.exists() {
            match compute_sha256(&local_path) {
                Ok(local_hash) => local_hash != shard_entry.sha256,
                Err(_) => true, // Error reading local file, re-download
            }
        } else {
            true
        };

        if !needs_download {
            tracing::debug!("Shard {} is up to date", shard_name);
            continue;
        }

        // Download shard
        tracing::info!("Downloading shard {} from {}", shard_name, origin_url);
        let shard_url = format!(
            "{}/{}/catalog/2/{}",
            origin_url, publisher, &shard_entry.sha256
        );
        let mut response = client.get(&shard_url).send()?;

        if !response.status().is_success() {
            return Err(ShardSyncError::new(format!(
                "Failed to download shard {}: HTTP {}",
                shard_name,
                response.status()
            )));
        }

        // Write to temporary file
        let temp_path = local_shard_dir.join(format!("{}.tmp", shard_name));
        let mut file = fs::File::create(&temp_path)?;
        response.copy_to(&mut file)?;
        drop(file);

        // Verify SHA-256
        let downloaded_hash = compute_sha256(&temp_path)?;
        if downloaded_hash != shard_entry.sha256 {
            fs::remove_file(&temp_path)?;
            return Err(ShardSyncError::new(format!(
                "SHA-256 mismatch for {}: expected {}, got {}",
                shard_name, shard_entry.sha256, downloaded_hash
            )));
        }

        // Atomic rename
        fs::rename(&temp_path, &local_path)?;
        tracing::info!("Successfully downloaded {}", shard_name);
    }

    // Write local copy of index for future comparisons
    let index_json = serde_json::to_string_pretty(&index)?;
    fs::write(local_shard_dir.join("catalog.attrs"), index_json)?;

    Ok(())
}

/// Compute SHA-256 hash of a file.
fn compute_sha256(path: &Path) -> Result<String, ShardSyncError> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
