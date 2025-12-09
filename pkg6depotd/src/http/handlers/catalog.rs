use axum::{
    extract::{Path, State, Request},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::errors::DepotError;
use tower_http::services::ServeFile;
use tower::ServiceExt;
use axum::http::header;

pub async fn get_catalog_v1(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, filename)): Path<(String, String)>,
    req: Request,
) -> Result<Response, DepotError> {
    let path = repo.get_catalog_file_path(&publisher, &filename)?;

    let service = ServeFile::new(path);
    let result = service.oneshot(req).await;
    
    match result {
        Ok(mut res) => {
            // Ensure correct content-type for JSON catalog artifacts regardless of file extension
            let is_catalog_json = filename == "catalog.attrs" || filename.starts_with("catalog.");
            if is_catalog_json {
                res.headers_mut().insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
            }
            Ok(res.into_response())
        },
        Err(e) => Err(DepotError::Server(e.to_string())),
    }
}
