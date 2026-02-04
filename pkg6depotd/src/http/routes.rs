use crate::http::admin;
use crate::http::handlers::{catalog, file, info, manifest, publisher, search, shard, versions};
use crate::repo::DepotRepo;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

pub fn app_router(state: Arc<DepotRepo>) -> Router {
    Router::new()
        .route("/versions/0", get(versions::get_versions))
        .route("/versions/0/", get(versions::get_versions))
        .route(
            "/{publisher}/catalog/1/{filename}",
            get(catalog::get_catalog_v1).head(catalog::get_catalog_v1),
        )
        .route(
            "/{publisher}/catalog/2/catalog.attrs",
            get(shard::get_shard_index).head(shard::get_shard_index),
        )
        .route(
            "/{publisher}/catalog/2/{sha256}",
            get(shard::get_shard_blob).head(shard::get_shard_blob),
        )
        .route(
            "/{publisher}/manifest/0/{fmri}",
            get(manifest::get_manifest).head(manifest::get_manifest),
        )
        .route(
            "/{publisher}/manifest/1/{fmri}",
            get(manifest::get_manifest).head(manifest::get_manifest),
        )
        .route(
            "/{publisher}/file/0/{algo}/{digest}",
            get(file::get_file).head(file::get_file),
        )
        .route(
            "/{publisher}/file/1/{algo}/{digest}",
            get(file::get_file).head(file::get_file),
        )
        .route(
            "/{publisher}/file/1/{digest}",
            get(file::get_file_no_algo).head(file::get_file_no_algo),
        )
        .route("/{publisher}/info/0/{fmri}", get(info::get_info))
        .route("/{publisher}/publisher/0", get(publisher::get_publisher_v0))
        .route(
            "/{publisher}/publisher/0/",
            get(publisher::get_publisher_v0),
        )
        .route("/{publisher}/publisher/1", get(publisher::get_publisher_v1))
        .route(
            "/{publisher}/publisher/1/",
            get(publisher::get_publisher_v1),
        )
        .route("/publisher/0", get(publisher::get_default_publisher_v0))
        .route("/publisher/0/", get(publisher::get_default_publisher_v0))
        .route("/publisher/1", get(publisher::get_default_publisher_v1))
        .route("/publisher/1/", get(publisher::get_default_publisher_v1))
        .route("/{publisher}/search/0/{token}", get(search::get_search_v0))
        .route("/{publisher}/search/1/{token}", get(search::get_search_v1))
        // Admin API over HTTP
        .route("/admin/health", get(admin::health))
        .route("/admin/auth/check", post(admin::auth_check))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
