use anyhow::Result;
use hub_api::routes::build_router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let addr = std::env::var("HUB_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_owned());
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "agentenv skills hub listening");
    axum::serve(listener, build_router()).await?;
    Ok(())
}
