use crate::errors::DepotError;
use crate::repo::DepotRepo;
use axum::{
    extract::{Path, Request, State},
    http::header,
    response::{IntoResponse, Response},
};
use httpdate::fmt_http_date;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub async fn get_file(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, _algo, digest)): Path<(String, String, String)>,
    req: Request,
) -> Result<Response, DepotError> {
    let path = repo.get_file_path(&publisher, &digest).ok_or_else(|| {
        DepotError::Repo(libips::repository::RepositoryError::NotFound(
            digest.clone(),
        ))
    })?;

    let service = ServeFile::new(path);
    let result = service.oneshot(req).await;

    match result {
        Ok(mut res) => {
            // Add caching headers
            let max_age = repo.cache_max_age();
            res.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_str(&format!("public, max-age={}", max_age)).unwrap(),
            );
            // ETag from digest
            res.headers_mut().insert(
                header::ETAG,
                header::HeaderValue::from_str(&format!("\"{}\"", digest)).unwrap(),
            );
            // Last-Modified from fs metadata
            if let Some(body_path) = res.extensions().get::<std::path::PathBuf>().cloned() {
                if let Ok(meta) = fs::metadata(&body_path) {
                    if let Ok(mtime) = meta.modified() {
                        let lm = fmt_http_date(mtime);
                        res.headers_mut().insert(
                            header::LAST_MODIFIED,
                            header::HeaderValue::from_str(&lm).unwrap(),
                        );
                    }
                }
            }
            // Fallback: use now if extension not present (should rarely happen)
            if !res.headers().contains_key(header::LAST_MODIFIED) {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|_| SystemTime::now())
                    .unwrap_or_else(SystemTime::now);
                let lm = fmt_http_date(now);
                res.headers_mut().insert(
                    header::LAST_MODIFIED,
                    header::HeaderValue::from_str(&lm).unwrap(),
                );
            }
            Ok(res.into_response())
        }
        Err(e) => Err(DepotError::Server(e.to_string())),
    }
}
