use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    http::header,
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::errors::DepotError;
use libips::fmri::Fmri;
use std::str::FromStr;
use sha1::Digest as _;

pub async fn get_manifest(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, fmri_str)): Path<(String, String)>,
) -> Result<Response, DepotError> {
    let fmri = Fmri::from_str(&fmri_str).map_err(|e| DepotError::Repo(libips::repository::RepositoryError::Other(e.to_string())))?;
    
    let content = repo.get_manifest_text(&publisher, &fmri)?;
    // Compute weak ETag from SHA-1 of manifest content (legacy friendly)
    let mut hasher = sha1::Sha1::new();
    hasher.update(content.as_bytes());
    let etag = format!("\"{}\"", format!("{:x}", hasher.finalize()));
    
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain"),
            (header::ETAG, etag.as_str()),
        ],
        content,
    ).into_response())
}
