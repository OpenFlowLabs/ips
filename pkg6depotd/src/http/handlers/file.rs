use axum::{
    extract::{Path, State, Request},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tower_http::services::ServeFile;
use tower::ServiceExt;
use crate::repo::DepotRepo;
use crate::errors::DepotError;

pub async fn get_file(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, _algo, digest)): Path<(String, String, String)>,
    req: Request,
) -> Result<Response, DepotError> {
    let path = repo.get_file_path(&publisher, &digest)
        .ok_or_else(|| DepotError::Repo(libips::repository::RepositoryError::NotFound(digest.clone())))?;

    let service = ServeFile::new(path);
    let result = service.oneshot(req).await;
    
    match result {
        Ok(res) => Ok(res.into_response()),
        Err(e) => Err(DepotError::Server(e.to_string())),
    }
}
