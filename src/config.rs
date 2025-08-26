use std::{collections::HashMap, path::PathBuf, str::FromStr};

use alloy_primitives::Address;
use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub rpcurl: String,
    #[serde(default)]
    pub actions: HashMap<String, ActionConfig>,
    #[serde(default)]
    pub l2: bool,
    #[serde(default)]
    pub event_sigs_path: Option<String>,
    #[serde(default)]
    pub func_sigs_path: Option<String>,
    #[serde(rename = "max-requests-per-second")]
    #[serde(default)]
    pub max_requests_per_second: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct ActionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub addresses: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub options: serde_json::Value,
}

pub fn load_config(path: &PathBuf) -> Result<Config> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file {}", path.display()))?;
    let cfg: Config = serde_json::from_str(&data).context("parsing config JSON")?;
    Ok(cfg)
}

pub fn collect_enabled_addresses(cfg: &Config) -> Result<Vec<Address>> {
    let mut set = std::collections::BTreeSet::new();
    for (_name, action) in cfg.actions.iter() {
        if action.enabled {
            for (addr_str, _props) in action.addresses.iter() {
                let addr = Address::from_str(addr_str)
                    .with_context(|| format!("invalid address in config: {addr_str}"))?;
                set.insert(addr);
            }
        }
    }
    if set.is_empty() {
        warn!("No enabled actions with addresses; filters will be empty");
    }
    Ok(set.into_iter().collect())
}
