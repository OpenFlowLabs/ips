use axum::{
    extract::{Path, State, Request},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::errors::DepotError;
use tower_http::services::ServeFile;
use tower::ServiceExt;

pub async fn get_catalog_v1(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, filename)): Path<(String, String)>,
    req: Request,
) -> Result<Response, DepotError> {
    let path = repo.get_legacy_catalog(&publisher, &filename)?;

    let service = ServeFile::new(path);
    let result = service.oneshot(req).await;
    
    match result {
        Ok(res) => Ok(res.into_response()),
        Err(e) => Err(DepotError::Server(e.to_string())),
    }
}
