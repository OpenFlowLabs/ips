//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tracing::{debug, instrument};

use super::catalog::{CatalogAttrs, CatalogPart, UpdateLog};
use super::{RepositoryError, Result};

fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::Digest as _;
    let mut hasher = sha1::Sha1::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| RepositoryError::DirectoryCreateError {
        path: parent.to_path_buf(),
        source: e,
    })?;

    let tmp: PathBuf = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| RepositoryError::FileWriteError {
            path: tmp.clone(),
            source: e,
        })?;
        f.write_all(bytes)
            .map_err(|e| RepositoryError::FileWriteError {
                path: tmp.clone(),
                source: e,
            })?;
        f.flush().map_err(|e| RepositoryError::FileWriteError {
            path: tmp.clone(),
            source: e,
        })?;
    }
    fs::rename(&tmp, path).map_err(|e| RepositoryError::FileWriteError {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

#[instrument(level = "debug", skip(attrs))]
pub(crate) fn write_catalog_attrs(path: &Path, attrs: &mut CatalogAttrs) -> Result<String> {
    // Compute signature over content without _SIGNATURE
    attrs.signature = None;
    let bytes_without_sig = serde_json::to_vec(&attrs).map_err(|e| {
        RepositoryError::JsonSerializeError(format!("Catalog attrs serialize error: {}", e))
    })?;
    let sig = sha1_hex(&bytes_without_sig);
    let mut sig_map = std::collections::HashMap::new();
    sig_map.insert("sha-1".to_string(), sig);
    attrs.signature = Some(sig_map);

    let final_bytes = serde_json::to_vec(&attrs).map_err(|e| {
        RepositoryError::JsonSerializeError(format!("Catalog attrs serialize error: {}", e))
    })?;
    debug!(path = %path.display(), bytes = final_bytes.len(), "writing catalog.attrs");
    atomic_write_bytes(path, &final_bytes)?;
    // safe to unwrap as signature was just inserted
    Ok(attrs
        .signature
        .as_ref()
        .and_then(|m| m.get("sha-1").cloned())
        .unwrap_or_default())
}

#[instrument(level = "debug", skip(part))]
pub(crate) fn write_catalog_part(path: &Path, part: &mut CatalogPart) -> Result<String> {
    // Compute signature over content without _SIGNATURE
    part.signature = None;
    let bytes_without_sig = serde_json::to_vec(&part).map_err(|e| {
        RepositoryError::JsonSerializeError(format!("Catalog part serialize error: {}", e))
    })?;
    let sig = sha1_hex(&bytes_without_sig);
    let mut sig_map = std::collections::HashMap::new();
    sig_map.insert("sha-1".to_string(), sig);
    part.signature = Some(sig_map);

    let final_bytes = serde_json::to_vec(&part).map_err(|e| {
        RepositoryError::JsonSerializeError(format!("Catalog part serialize error: {}", e))
    })?;
    debug!(path = %path.display(), bytes = final_bytes.len(), "writing catalog part");
    atomic_write_bytes(path, &final_bytes)?;
    Ok(part
        .signature
        .as_ref()
        .and_then(|m| m.get("sha-1").cloned())
        .unwrap_or_default())
}

#[instrument(level = "debug", skip(log))]
pub(crate) fn write_update_log(path: &Path, log: &mut UpdateLog) -> Result<String> {
    // Compute signature over content without _SIGNATURE
    log.signature = None;
    let bytes_without_sig = serde_json::to_vec(&log).map_err(|e| {
        RepositoryError::JsonSerializeError(format!("Update log serialize error: {}", e))
    })?;
    let sig = sha1_hex(&bytes_without_sig);
    let mut sig_map = std::collections::HashMap::new();
    sig_map.insert("sha-1".to_string(), sig);
    log.signature = Some(sig_map);

    let final_bytes = serde_json::to_vec(&log).map_err(|e| {
        RepositoryError::JsonSerializeError(format!("Update log serialize error: {}", e))
    })?;
    debug!(path = %path.display(), bytes = final_bytes.len(), "writing update log");
    atomic_write_bytes(path, &final_bytes)?;
    Ok(log
        .signature
        .as_ref()
        .and_then(|m| m.get("sha-1").cloned())
        .unwrap_or_default())
}
