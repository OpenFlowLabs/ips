use miette::Diagnostic;
use thiserror::Error;

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

pub type Result<T> = std::result::Result<T, DepotError>;
