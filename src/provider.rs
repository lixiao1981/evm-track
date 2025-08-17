use alloy_provider::{ProviderBuilder, RootProvider,Provider};
use alloy_transport::BoxTransport;
use anyhow::{Context, Result, anyhow};
use alloy_primitives::B256;
use alloy_rpc_types::TransactionReceipt;

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

pub async fn connect_ipc(path: &str) -> Result<RootProvider<BoxTransport>> {
    if !path.starts_with('/') {
        return Err(anyhow!("IPC path must be an absolute path, got {path}"));
    }
    let provider = ProviderBuilder::new()
        .on_builtin(path)
        .await
        .context("connecting IPC")?;
    Ok(provider)
}

pub async fn public_provider_get_receipt(tx_hash: B256) -> Result<Option<TransactionReceipt>> {
    let rpc_url = "https://bsc-dataseed.binance.org/";
    let provider: RootProvider<BoxTransport> = ProviderBuilder::new()
        .on_builtin(rpc_url)
        .await?;
    provider
        .get_transaction_receipt(tx_hash)
        .await
        .map_err(anyhow::Error::from)
}
