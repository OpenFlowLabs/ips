use axum::{
    extract::{Path, State, Request},
    response::{IntoResponse, Response},
    http::header,
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::errors::DepotError;
use tower_http::services::ServeFile;
use tower::ServiceExt;

pub async fn get_catalog(
    State(repo): State<Arc<DepotRepo>>,
    Path(publisher): Path<String>,
) -> Result<Response, DepotError> {
    let content = repo.get_legacy_catalog(&publisher)?;
    
    Ok((
        [(header::CONTENT_TYPE, "text/plain")],
        content
    ).into_response())
}

pub async fn get_catalog_v1(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, filename)): Path<(String, String)>,
    req: Request,
) -> Result<Response, DepotError> {
    let path = repo.get_catalog_file_path(&publisher, &filename)
        .ok_or_else(|| DepotError::Repo(libips::repository::RepositoryError::NotFound(filename.clone())))?;

    let service = ServeFile::new(path);
    let result = service.oneshot(req).await;
    
    match result {
        Ok(res) => Ok(res.into_response()),
        Err(e) => Err(DepotError::Server(e.to_string())),
    }
}
