use alloy_provider::{ProviderBuilder, RootProvider};
use alloy_transport::BoxTransport;
use anyhow::{Context, Result, anyhow};

// Connect using the built-in connection string API and return a boxed transport
pub async fn connect_ws(url: &str) -> Result<RootProvider<BoxTransport>> {
    if !url.starts_with("ws") {
        return Err(anyhow!("rpcurl must be a websocket (ws/wss), got {url}"));
    }
    let provider = ProviderBuilder::new()
        .on_builtin(url)
        .await
        .context("connecting websocket")?;
    Ok(provider)
}
