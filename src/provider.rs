use alloy_provider::{ProviderBuilder, RootProvider,Provider};
use alloy_transport::BoxTransport;
use std::sync::Arc;
use anyhow::{Context, Result, anyhow};
use alloy_primitives::B256;
use alloy_rpc_types::{TransactionReceipt};
use alloy::primitives::b256;
use alloy_rpc_types::trace::geth::{CallFrame, GethDebugTracingOptions};

// Connect using the built-in connection string API and return a boxed transport
pub async fn connect_auto(url: &str) -> Result<RootProvider<BoxTransport>> {
    let provider = ProviderBuilder::new()
        .on_builtin(url)
        .await
        .context(format!("Failed to connect to provider at {url}"))?;
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

pub async fn public_provider_get_transactions_trace(
    provider: Arc<RootProvider<BoxTransport>>,
    tx_hash: B256,
    options: GethDebugTracingOptions,
) -> Result<Option<CallFrame>> {
    let params = serde_json::json!([format!("0x{:x}", tx_hash), options]);
    let result: serde_json::Value = provider.client()
        .request("debug_traceTransaction", params)
        .await
        .map_err(anyhow::Error::from)?;

    let trace = serde_json::from_value(result).map_err(anyhow::Error::from)?;
    Ok(trace)
}

     