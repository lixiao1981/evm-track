use std::fs::OpenOptions;
use std::io::Write;

use super::{Action, EventRecord};
use crate::abi::DecodedValue;
use crate::error::AppError;

// Minimal TornadoCash-like detector with optional file output
#[derive(Clone, Default)]
pub struct TornadoOptions {
    pub output_filepath: Option<String>,
    pub verbose: bool,
}

pub struct TornadoAction {
    opts: TornadoOptions,
}

impl TornadoAction {
    pub fn new(opts: TornadoOptions) -> Self {
        if opts.verbose {
            if let Some(path) = &opts.output_filepath {
                println!("[DEBUG] TornadoAction: output file path = {}", path);
            } else {
                println!("[DEBUG] TornadoAction: no output file path specified");
            }
        }
        TornadoAction { opts }
    }

    /// 查找事件中的金额字段
    fn find_amount_field(&self, e: &EventRecord) -> String {
        // 常见的金额字段名称
        let amount_fields = ["wad", "amount", "value", "tokens"];
        
        for field in &e.fields {
            if amount_fields.contains(&field.name.as_str()) {
                match &field.value {
                    DecodedValue::Uint(amount) => {
                        // 转换为BNB单位显示 (除以 10^18) - BSC网络使用BNB
                        let bnb_amount = format_wei_to_bnb(amount);
                        return format!("{} WEI ({} BNB)", amount, bnb_amount);
                    }
                    _ => continue,
                }
            }
        }
        
        // 如果没找到具体字段，尝试显示所有字段用于调试
        if !e.fields.is_empty() {
            let mut field_info = Vec::new();
            for field in &e.fields {
                match &field.value {
                    DecodedValue::Uint(val) => field_info.push(format!("{}={}", field.name, val)),
                    DecodedValue::Address(addr) => field_info.push(format!("{}={}", field.name, addr)),
                    _ => {}
                }
            }
            return field_info.join(", ");
        }
        
        "unknown".to_string()
    }
}

impl Action for TornadoAction {
    fn on_event(&self, e: &EventRecord) -> Result<(), AppError> {
        // 只处理真正的TornadoCash合约地址
        // 这些是已知的TornadoCash合约地址（主要是以太坊网络的）
        // BSC上的TornadoCash合约地址应根据实际情况添加
        let known_tornado_addresses = vec![
            "0x12d66f87a04a9e220743712ce6d9bb1b5616b8fc", // Tornado.cash ETH 0.1
            "0x47ce0c6ed5b0ce3d3a51fdb1c52dc66a7c3c2936", // Tornado.cash ETH 1
            "0x910cbd523d972eb0a6f4cae4618ad62622b39dbf", // Tornado.cash ETH 10
            "0xa160cdab225685da1d56aa342ad8841c3b53f291", // Tornado.cash ETH 100
            // 添加更多已知地址...
        ];
        
        // 检查是否是已知的TornadoCash合约地址
        let is_tornado_contract = known_tornado_addresses.iter().any(|addr| {
            if let Ok(tornado_addr) = addr.parse::<alloy_primitives::Address>() {
                tornado_addr == e.address
            } else {
                false
            }
        });
        
        // 如果不是已知的TornadoCash合约，就跳过
        if !is_tornado_contract {
            return Ok(());
        }
        
        if let Some(name) = &e.name {
            match name.as_str() {
                "Deposit" | "Deposited" => {
                    // 查找金额字段 (wad 或 amount)
                    let amount = self.find_amount_field(e);
                    let line = format!(
                        "[tornado] deposit addr={} tx={:?} block={:?} amount={}",
                        e.address, e.tx_hash, e.block_number, amount
                    );
                    println!("{}", line);
                    if let Some(path) = &self.opts.output_filepath {
                        if self.opts.verbose {
                            println!("[DEBUG] Writing deposit to file: {}", path);
                        }
                        match append_line(path, &line) {
                            Ok(_) => {
                                if self.opts.verbose {
                                    println!("[DEBUG] Successfully wrote deposit to file");
                                }
                            }
                            Err(e) => {
                                if self.opts.verbose {
                                    println!("[DEBUG] Failed to write deposit to file: {}", e);
                                }
                            }
                        }
                    }
                }
                "Withdrawal" | "Withdraw" => {
                    // 查找金额字段 (wad 或 amount)
                    let amount = self.find_amount_field(e);
                    let line = format!(
                        "[tornado] withdrawal addr={} tx={:?} block={:?} amount={}",
                        e.address, e.tx_hash, e.block_number, amount
                    );
                    println!("{}", line);
                    if let Some(path) = &self.opts.output_filepath {
                        if self.opts.verbose {
                            println!("[DEBUG] Writing withdrawal to file: {}", path);
                        }
                        match append_line(path, &line) {
                            Ok(_) => {
                                if self.opts.verbose {
                                    println!("[DEBUG] Successfully wrote withdrawal to file");
                                }
                            }
                            Err(e) => {
                                if self.opts.verbose {
                                    println!("[DEBUG] Failed to write withdrawal to file: {}", e);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn append_line(path: &str, s: &str) -> std::io::Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", s)?;
    Ok(())
}

/// 将Wei转换为BNB格式显示 (BSC网络)
fn format_wei_to_bnb(wei: &alloy_primitives::U256) -> String {
    use alloy_primitives::U256;
    
    let bnb_unit = U256::from(1_000_000_000_000_000_000u64); // 10^18
    let bnb_amount = wei / bnb_unit;
    let remainder = wei % bnb_unit;
    
    if remainder.is_zero() {
        format!("{}", bnb_amount)
    } else {
        // 计算小数部分 (最多显示6位小数)
        let remainder_scaled = remainder * U256::from(1_000_000u64) / bnb_unit;
        if remainder_scaled.is_zero() {
            format!("{}", bnb_amount)
        } else {
            format!("{}.{:06}", bnb_amount, remainder_scaled.to::<u64>())
        }
    }
}
