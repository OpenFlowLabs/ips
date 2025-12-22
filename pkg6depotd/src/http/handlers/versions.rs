use axum::response::IntoResponse;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Info,
    Versions,
    Catalog,
    Manifest,
    File,
    Publisher,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Operation::Info => "info",
            Operation::Versions => "versions",
            Operation::Catalog => "catalog",
            Operation::Manifest => "manifest",
            Operation::File => "file",
            Operation::Publisher => "publisher",
        };
        write!(f, "{}", s)
    }
}

pub struct SupportedOperation {
    pub op: Operation,
    pub versions: Vec<u32>,
}

pub struct VersionsResponse {
    pub server_version: String,
    pub operations: Vec<SupportedOperation>,
}

impl fmt::Display for VersionsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "pkg-server {}", self.server_version)?;
        for op in &self.operations {
            write!(f, "{}", op.op)?;
            for v in &op.versions {
                write!(f, " {}", v)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

pub async fn get_versions() -> impl IntoResponse {
    let pkg_version = env!("CARGO_PKG_VERSION");
    let server_version = format!("pkg6depotd-{}", pkg_version);

    let response = VersionsResponse {
        server_version,
        operations: vec![
            SupportedOperation {
                op: Operation::Info,
                versions: vec![0],
            },
            SupportedOperation {
                op: Operation::Versions,
                versions: vec![0],
            },
            SupportedOperation {
                op: Operation::Catalog,
                versions: vec![1],
            },
            SupportedOperation {
                op: Operation::Manifest,
                versions: vec![0, 1],
            },
            SupportedOperation {
                op: Operation::File,
                versions: vec![0, 1],
            },
            SupportedOperation {
                op: Operation::Publisher,
                versions: vec![0, 1],
            },
        ],
    };

    response.to_string()
}
