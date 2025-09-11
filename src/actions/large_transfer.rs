use super::{Action, EventRecord};
use crate::error::Result;
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeTransferOptions {
    #[serde(rename = "threshold")]
    pub min_amount_human: Option<String>,
    
    #[serde(rename = "decimals-default")]
    pub decimals_default: u8,
    
    #[serde(skip)]
    pub verbose: bool,
}

pub struct LargeTransferAction {
    opts: LargeTransferOptions,
}

impl LargeTransferAction {
    pub fn new(opts: LargeTransferOptions, verbose: bool) -> Self {
        let mut opts = opts;
        opts.verbose = verbose;
        Self { opts }
    }
    
    /// 根据合约地址获取正确的代币精度
    fn get_token_decimals(&self, address: &Address) -> u8 {
        let formatted_addr = format!("{:#x}", address).to_lowercase();
        
        // 使用BSC主网上已知代币的精度
        let decimals = match formatted_addr.as_str() {
            // NOTE: USDT on BSC uses 18 decimals (align with on-chain contract response)
            "0x55d398326f99059ff775485246999027b3197955" => 18,  // USDT
            "0x8ac76a51cc950d9822d68b83fe1ad97b32cd580d" => 6,  // USDC  
            "0xe9e7cea3dedca5984780bafc599bd69add087d56" => 18, // BUSD
            "0x2170ed0880ac9a755fd29b2688956bd959f933f8" => 18, // ETH
            "0x7130d2a12b9bcbfae4f2634d864a1ee1ce3ead9c" => 18, // BTCB
            "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c" => 18, // WBNB
            _ => self.opts.decimals_default,
        };
        
        decimals
    }
    
    /// 格式化数值为人类可读格式
    fn format_amount(&self, amount: U256, decimals: u8) -> String {
        let divisor = pow10_u256(decimals as usize);
        let integer_part = amount / divisor;
        let fractional_part = amount % divisor;
        
        if fractional_part == U256::ZERO {
            integer_part.to_string()
        } else {
            // 格式化小数部分，去除尾随零
            let frac_str = format!("{:0width$}", fractional_part, width = decimals as usize);
            let frac_trimmed = frac_str.trim_end_matches('0');
            if frac_trimmed.is_empty() {
                integer_part.to_string()
            } else {
                format!("{}.{}", integer_part, frac_trimmed)
            }
        }
    }
}

fn parse_uint_dec(s: &str) -> Option<U256> {
    let mut acc = U256::from(0);
    for ch in s.chars() {
        let d = ch.to_digit(10)? as u64;
        acc = acc * U256::from(10u8) + U256::from(d);
    }
    Some(acc)
}

fn pow10_u256(n: usize) -> U256 {
    let mut acc = U256::from(1);
    for _ in 0..n {
        acc = acc * U256::from(10u8);
    }
    acc
}

fn parse_human_to_u256(s: &str, decimals: u8) -> Option<U256> {
    if let Some((int_part, frac_part)) = s.split_once('.') {
        let int_v = parse_uint_dec(int_part)?;
        if frac_part.len() as u8 > decimals {
            return None;
        }
        let denom = pow10_u256(decimals as usize);
        let scale = pow10_u256(frac_part.len());
        let frac_v = parse_uint_dec(frac_part)?;
        let frac_scaled = (denom / scale) * frac_v;
        Some(int_v * denom + frac_scaled)
    } else {
        let int_v = parse_uint_dec(s)?;
        let denom = pow10_u256(decimals as usize);
        Some(int_v * denom)
    }
}

impl Action for LargeTransferAction {
    fn on_event(&self, record: &EventRecord) -> Result<()> {        
        if self.opts.verbose {
            println!("DEBUG: LargeTransferAction received event at {:?}, topics length: {}", 
                record.address, record.topics.len());
            if !record.topics.is_empty() {
                println!("DEBUG: First topic: {}", record.topics[0]);
            }
        }
        
        // 检查是否为 Transfer 事件
        if record.topics.len() >= 3 {
            let transfer_sig = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
            let actual_topic = record.topics[0].to_string();
            if self.opts.verbose {
                println!("DEBUG: Comparing topics - actual: {}, expected: {}", actual_topic, transfer_sig);
            }
            if actual_topic == transfer_sig {
                
                // 从字段中提取 Transfer 金额 (value 是第三个字段，索引为2)
                let amount = if let Some(field) = record.fields.get(2) {
                    match &field.value {
                        crate::abi::DecodedValue::Uint(val) => *val,
                        _ => return Ok(()),
                    }
                } else {
                    return Ok(());
                };
                
                // 获取正确的代币精度
                let decimals = self.get_token_decimals(&record.address);
                
                // 如果设置了最小金额阈值，检查是否超过阈值
                if let Some(min_amount_str) = &self.opts.min_amount_human {
                    if let Some(min_amount) = parse_human_to_u256(min_amount_str, decimals) {
                        if self.opts.verbose {
                            println!("DEBUG: Transfer amount: {}, threshold: {}, decimals: {}", amount, min_amount, decimals);
                        }
                        if amount < min_amount {
                            return Ok(());
                        }
                    } else {
                        if self.opts.verbose {
                            println!("DEBUG: Failed to parse threshold: {}", min_amount_str);
                        }
                        return Ok(());
                    }
                } else {
                    if self.opts.verbose {
                        println!("DEBUG: No threshold configured");
                    }
                }

                // 异常数值检测（例如远超常规供应量的可疑转账）: 默认 > 1e13 直接标记异常并忽略
                // 1e13 以人类单位（decimals 之后）表示，这里转换成整数判断
                if let Some(anomaly_threshold) = parse_human_to_u256("10000000000000", decimals) { // 10,000,000,000,000
                    if amount > anomaly_threshold {
                        let formatted_amount = self.format_amount(amount, decimals);
                        if self.opts.verbose {
                            println!("ANOMALY Large Transfer (ignored): {} tokens at {:?} (decimals: {})", 
                                formatted_amount, record.address, decimals);
                            println!("  Block: {}, Tx: {:?}", 
                                record.block_number.unwrap_or(0), record.tx_hash);
                        }
                        return Ok(());
                    }
                }
                
                // 格式化金额显示
                let formatted_amount = self.format_amount(amount, decimals);
                
                if self.opts.verbose {
                    println!("Large Transfer: {} tokens at {:?} (decimals: {})", 
                        formatted_amount, record.address, decimals);
                    println!("  Block: {}, Tx: {:?}", 
                        record.block_number.unwrap_or(0), record.tx_hash);
                }
            }
        }
        Ok(())
    }
}
