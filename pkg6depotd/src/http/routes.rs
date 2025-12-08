use axum::{
    routing::get,
    Router,
};
use crate::http::handlers::versions;

pub fn app_router() -> Router {
    Router::new()
        .route("/versions/0/", get(versions::get_versions))
}
