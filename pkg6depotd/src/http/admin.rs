use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::Arc;

use crate::repo::DepotRepo;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub async fn health(_state: State<Arc<DepotRepo>>) -> impl IntoResponse {
    // Basic liveness/readiness for now. Future: include repo checks.
    (StatusCode::OK, Json(HealthResponse { status: "ok" }))
}

#[derive(Serialize)]
struct AuthCheckResponse<'a> {
    authenticated: bool,
    token_present: bool,
    subject: Option<&'a str>,
    scopes: Vec<&'a str>,
    decision: &'static str,
}

/// Admin auth-check endpoint.
/// For now, this is a minimal placeholder that only checks for the presence of a Bearer token.
/// TODO: Validate JWT via OIDC JWKs using configured issuer/jwks_uri and required scopes.
pub async fn auth_check(_state: State<Arc<DepotRepo>>, headers: HeaderMap) -> Response {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let (authenticated, token_present) = match auth {
        Some(h) if h.to_ascii_lowercase().starts_with("bearer ") => (true, true),
        Some(_) => (false, true),
        None => (false, false),
    };

    let resp = AuthCheckResponse {
        authenticated,
        token_present,
        subject: None,
        scopes: vec![],
        decision: if authenticated { "allow" } else { "deny" },
    };

    let status = if authenticated {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    };
    (status, Json(resp)).into_response()
}
