use crate::errors::Result;
use axum::Router;
use tokio::net::TcpListener;

pub async fn run(router: Router, listener: TcpListener) -> Result<()> {
    let addr = listener.local_addr()?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, router)
        .await
        .map_err(|e| crate::errors::DepotError::Server(e.to_string()))
}
