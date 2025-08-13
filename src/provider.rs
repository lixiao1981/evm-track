use anyhow::{anyhow, Context, Result};
use alloy_provider::{ProviderBuilder, RootProvider};
use alloy_transport_ws::{WsClient, WsConnect};

pub async fn connect_ws(url: &str) -> Result<RootProvider<WsClient>> {
    if !url.starts_with("ws") {
        return Err(anyhow!("rpcurl must be a websocket (ws/wss), got {url}"));
    }
    let ws = WsConnect::new(url);
    let provider = ProviderBuilder::new()
        .on_ws(ws)
        .await
        .context("connecting websocket")?;
    Ok(provider)
}

