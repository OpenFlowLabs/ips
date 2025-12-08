use tokio::net::TcpListener;
use axum::Router;
use std::net::SocketAddr;
use crate::errors::Result;

pub async fn run(router: Router, bind_addr: &str) -> Result<()> {
    let addr: SocketAddr = bind_addr.parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);
    
    axum::serve(listener, router).await.map_err(|e| crate::errors::DepotError::Server(e.to_string()))
}
