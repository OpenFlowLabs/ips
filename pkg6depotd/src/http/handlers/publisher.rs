use crate::errors::DepotError;
use crate::repo::DepotRepo;
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
struct P5iPublisherInfo {
    alias: Option<String>,
    name: String,
    packages: Vec<String>,
    repositories: Vec<String>,
}

#[derive(Serialize)]
struct P5iFile {
    packages: Vec<String>,
    publishers: Vec<P5iPublisherInfo>,
    version: u32,
}

pub async fn get_publisher_v0(
    state: State<Arc<DepotRepo>>,
    Path(publisher): Path<String>,
) -> Result<Response, DepotError> {
    get_publisher_response(state, Some(publisher)).await
}

pub async fn get_publisher_v1(
    state: State<Arc<DepotRepo>>,
    Path(publisher): Path<String>,
) -> Result<Response, DepotError> {
    get_publisher_response(state, Some(publisher)).await
}

pub async fn get_default_publisher_v0(
    state: State<Arc<DepotRepo>>,
) -> Result<Response, DepotError> {
    get_publisher_response(state, None).await
}

pub async fn get_default_publisher_v1(
    state: State<Arc<DepotRepo>>,
) -> Result<Response, DepotError> {
    get_publisher_response(state, None).await
}

async fn get_publisher_response(
    State(repo): State<Arc<DepotRepo>>,
    publisher: Option<String>,
) -> Result<Response, DepotError> {
    let repo_info = repo.get_info()?;

    if let Some(name) = publisher {
        let pub_info = repo_info.publishers.into_iter().find(|p| p.name == name);

        if let Some(p) = pub_info {
            let p5i = P5iFile {
                packages: Vec::new(),
                publishers: vec![P5iPublisherInfo {
                    alias: None,
                    name: p.name,
                    packages: Vec::new(),
                    repositories: Vec::new(),
                }],
                version: 1,
            };
            let json = serde_json::to_string_pretty(&p5i)
                .map_err(|e| DepotError::Server(e.to_string()))?;
            Ok(([(header::CONTENT_TYPE, "application/vnd.pkg5.info")], json).into_response())
        } else {
            Err(DepotError::Repo(
                libips::repository::RepositoryError::PublisherNotFound(name),
            ))
        }
    } else {
        // Return all publishers
        let publishers = repo_info
            .publishers
            .into_iter()
            .map(|p| P5iPublisherInfo {
                alias: None,
                name: p.name,
                packages: Vec::new(),
                repositories: Vec::new(),
            })
            .collect();

        let p5i = P5iFile {
            packages: Vec::new(),
            publishers,
            version: 1,
        };
        let json =
            serde_json::to_string_pretty(&p5i).map_err(|e| DepotError::Server(e.to_string()))?;
        Ok(([(header::CONTENT_TYPE, "application/vnd.pkg5.info")], json).into_response())
    }
}
