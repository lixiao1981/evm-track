use alloy_provider::{ProviderBuilder, RootProvider, Provider};
use alloy_primitives::B256;
use alloy_rpc_types::TransactionReceipt;
use alloy_transport::BoxTransport;
use anyhow::Result;

/// Fetches a transaction receipt from the given RPC URL (default: BSC public dataseed).
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
