use axum::response::IntoResponse;

pub async fn get_versions() -> impl IntoResponse {
    // According to pkg5 depot docs: text/plain list of supported ops and versions.
    // "pkg-server <version>\ninfo 0\n..."
    let version_str = "pkg-server pkg6depotd-0.1\n\
                       info 0\n\
                       search 0\n\
                       versions 0\n\
                       catalog 0\n\
                       manifest 0\n\
                       file 0\n";
    
    version_str.to_string()
}
