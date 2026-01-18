use crate::errors::DepotError;
use crate::repo::DepotRepo;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

pub async fn get_search_v0(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, token)): Path<(String, String)>,
) -> Result<Response, DepotError> {
    // Decode the token (it might be URL encoded in the path, but axum usually decodes path params?
    // Actually, axum decodes percent-encoded path segments automatically if typed as String?
    // Let's assume yes or use it as is.
    // However, typical search tokens might contain chars that need decoding.
    // If standard axum decoding is not enough, we might need manual decoding.
    // But let's start with standard.

    // Call search
    let results = repo.search(Some(&publisher), &token, false)?;

    // Format output: index action value package
    let mut body = String::new();
    for entry in results {
        body.push_str(&format!(
            "{} {} {} {}\n",
            entry.index_type, entry.action_type, entry.value, entry.fmri
        ));
    }

    Ok(([(axum::http::header::CONTENT_TYPE, "text/plain")], body).into_response())
}

pub async fn get_search_v1(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, token)): Path<(String, String)>,
) -> Result<Response, DepotError> {
    // Search v1 token format: "<case>_<rtype>_<trans>_<installroot>_<query>"
    // Example: "False_2_None_None_%3A%3A%3Apostgres" -> query ":::postgres"
    let (prefix, query) = if let Some((p, q)) = split_v1_token(&token) {
        (p, q)
    } else {
        ("False_2_None_None", token.as_str())
    };

    // Parse prefix fields
    let parts: Vec<&str> = prefix.split('_').collect();
    let case_sensitive = parts.get(0).map(|s| *s == "True").unwrap_or(false);
    let p1 = if case_sensitive { "1" } else { "0" }; // query number/flag
    let p2 = parts.get(1).copied().unwrap_or("2"); // return type

    // Run search with provided publisher and query
    let results = repo.search(Some(&publisher), query, case_sensitive)?;

    // No results -> 204 No Content per v1 spec
    if results.is_empty() {
        return Ok((axum::http::StatusCode::NO_CONTENT).into_response());
    }

    // Format: "p1 p2 <fmri> <index_type> <action_type> <value> [k=v ...]"
    let mut body = String::from("Return from search v1\n");
    for entry in results {
        let mut line = format!(
            "{} {} {} {} {} {}",
            p1, p2, entry.fmri, entry.index_type, entry.action_type, entry.value
        );
        // Attributes are already in a BTreeMap, so iteration order is stable
        for (k, v) in &entry.attributes {
            line.push_str(&format!(" {}={}", k, v));
        }
        line.push('\n');
        body.push_str(&line);
    }

    Ok(([(axum::http::header::CONTENT_TYPE, "text/plain")], body).into_response())
}

fn split_v1_token(token: &str) -> Option<(&str, &str)> {
    // Try to find the 4th underscore
    let mut parts = token.splitn(5, '_');
    if let (Some(_), Some(_), Some(_), Some(_), Some(_)) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        // We found 4 parts and a remainder.
        // We need to reconstruct where the split happened to return slices
        // Actually, splitn(5) returns 5 parts. The last part is the remainder.
        // But we want to be careful about the length of the prefix.

        // Let's iterate chars to find 4th underscore
        let mut underscore_count = 0;
        for (i, c) in token.chars().enumerate() {
            if c == '_' {
                underscore_count += 1;
                if underscore_count == 4 {
                    return Some((&token[..i], &token[i + 1..]));
                }
            }
        }
    }
    None
}
