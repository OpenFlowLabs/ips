use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::http::handlers::{versions, catalog, manifest, file, info};

pub fn app_router(state: Arc<DepotRepo>) -> Router {
    Router::new()
        .route("/versions/0/", get(versions::get_versions))
        .route("/{publisher}/catalog/0/", get(catalog::get_catalog))
        .route("/{publisher}/manifest/0/{fmri}", get(manifest::get_manifest))
        .route("/{publisher}/file/0/{algo}/{digest}", get(file::get_file))
        .route("/{publisher}/info/0/{fmri}", get(info::get_info))
        .with_state(state)
}
