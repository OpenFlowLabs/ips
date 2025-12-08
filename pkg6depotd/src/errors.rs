use miette::Diagnostic;
use thiserror::Error;
use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
};

#[derive(Error, Debug, Diagnostic)]
pub enum DepotError {
    #[error("Configuration error: {0}")]
    #[diagnostic(code(ips::depot_error::config))]
    Config(String),

    #[error("IO error: {0}")]
    #[diagnostic(code(ips::depot_error::io))]
    Io(#[from] std::io::Error),

    #[error("Address parse error: {0}")]
    #[diagnostic(code(ips::depot_error::addr_parse))]
    AddrParse(#[from] std::net::AddrParseError),

    #[error("Server error: {0}")]
    #[diagnostic(code(ips::depot_error::server))]
    Server(String),
    
    #[error("Repository error: {0}")]
    #[diagnostic(code(ips::depot_error::repo))]
    Repo(#[from] libips::repository::RepositoryError),
}

impl IntoResponse for DepotError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            DepotError::Repo(libips::repository::RepositoryError::NotFound(_)) => (StatusCode::NOT_FOUND, self.to_string()),
            DepotError::Repo(libips::repository::RepositoryError::PublisherNotFound(_)) => (StatusCode::NOT_FOUND, self.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        
        (status, message).into_response()
    }
}

pub type Result<T> = std::result::Result<T, DepotError>;
