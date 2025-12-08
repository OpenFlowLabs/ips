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

pub async fn get_manifest(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, fmri_str)): Path<(String, String)>,
) -> Result<Response, DepotError> {
    let fmri = Fmri::from_str(&fmri_str).map_err(|e| DepotError::Repo(libips::repository::RepositoryError::Other(e.to_string())))?;
    
    let content = repo.get_manifest_text(&publisher, &fmri)?;
    
    Ok((
        [(header::CONTENT_TYPE, "text/plain")],
        content
    ).into_response())
}
