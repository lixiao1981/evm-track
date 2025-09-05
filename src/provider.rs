use crate::error::{AppError, Result};
use alloy_primitives::B256;
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use alloy_rpc_types::trace::geth::{CallFrame, GethDebugTracingOptions};
use alloy_rpc_types::TransactionReceipt;
use alloy_transport::BoxTransport;
use std::sync::Arc;

// Connect using the built-in connection string API and return a boxed transport
pub async fn connect_auto(url: &str) -> Result<RootProvider<BoxTransport>> {
    let provider = ProviderBuilder::new().on_builtin(url).await?;
    Ok(provider)
}

pub async fn public_provider_get_receipt(
    tx_hash: B256,
) -> Result<Option<TransactionReceipt>> {
    let rpc_url = "https://bsc-dataseed.binance.org/";
    let provider: RootProvider<BoxTransport> = connect_auto(rpc_url).await?;
    Ok(provider.get_transaction_receipt(tx_hash).await?)
}

pub async fn public_provider_get_transactions_trace(
    provider: Arc<RootProvider<BoxTransport>>,
    tx_hash: B256,
    options: GethDebugTracingOptions,
) -> Result<Option<CallFrame>> {
    let params = serde_json::json!([format!("0x{:x}", tx_hash), options]);
    let result: serde_json::Value = provider.client().request("debug_traceTransaction", params).await?;
    let trace = serde_json::from_value(result)?;
    Ok(trace)
}

     