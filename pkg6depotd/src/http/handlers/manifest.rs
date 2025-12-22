use crate::errors::DepotError;
use crate::repo::DepotRepo;
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use libips::fmri::Fmri;
use sha1::Digest as _;
use std::str::FromStr;
use std::sync::Arc;

pub async fn get_manifest(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, fmri_str)): Path<(String, String)>,
) -> Result<Response, DepotError> {
    let fmri = Fmri::from_str(&fmri_str)
        .map_err(|e| DepotError::Repo(libips::repository::RepositoryError::Other(e.to_string())))?;

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
    )
        .into_response())
}
