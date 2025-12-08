use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::http::handlers::{versions, catalog, manifest, file, info, publisher};

pub fn app_router(state: Arc<DepotRepo>) -> Router {
    Router::new()
        .route("/versions/0/", get(versions::get_versions))
        .route("/{publisher}/catalog/0/", get(catalog::get_catalog))
        .route("/{publisher}/catalog/1/{filename}", get(catalog::get_catalog_v1))
        .route("/{publisher}/manifest/0/{fmri}", get(manifest::get_manifest))
        .route("/{publisher}/manifest/1/{fmri}", get(manifest::get_manifest))
        .route("/{publisher}/file/0/{algo}/{digest}", get(file::get_file))
        .route("/{publisher}/file/1/{algo}/{digest}", get(file::get_file))
        .route("/{publisher}/info/0/{fmri}", get(info::get_info))
        .route("/{publisher}/publisher/0", get(publisher::get_publisher_v0))
        .route("/{publisher}/publisher/1", get(publisher::get_publisher_v1))
        .with_state(state)
}
