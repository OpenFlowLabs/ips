use tokio::net::TcpListener;
use axum::Router;
use crate::errors::Result;

pub async fn run(router: Router, listener: TcpListener) -> Result<()> {
    let addr = listener.local_addr()?;
    tracing::info!("Listening on {}", addr);
    
    axum::serve(listener, router).await.map_err(|e| crate::errors::DepotError::Server(e.to_string()))
}
