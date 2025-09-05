use super::{Action, EventRecord};
use crate::error::Result;
use alloy_primitives::U256;

#[derive(Clone, Default)]
pub struct LargeTransferOptions {
    pub min_amount_human: Option<String>,
    pub decimals_default: u8,
}

pub struct LargeTransferAction {
    opts: LargeTransferOptions,
}

impl LargeTransferAction {
    pub fn new(opts: LargeTransferOptions) -> Self {
        Self { opts }
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
    fn on_event(&self, e: &EventRecord) -> Result<()> {
        if e.name.as_deref() == Some("Transfer") {
            let mut amount_u256: Option<U256> = None;
            for f in &e.fields {
                if matches!(f.name.as_str(), "value" | "amount") {
                    if let crate::abi::DecodedValue::Uint(u) = &f.value {
                        amount_u256 = Some(*u);
                    }
                }
            }
            if let (Some(min_h), Some(amount)) = (&self.opts.min_amount_human, amount_u256) {
                let threshold =
                    parse_human_to_u256(min_h, self.opts.decimals_default).unwrap_or(U256::ZERO);
                if amount >= threshold {
                    println!(
                        "[alert-large-transfer] contract={} value_raw={} threshold(human)={} (dec={})",
                        e.address,
                        amount,
                        min_h,
                        self.opts.decimals_default
                    );
                }
            }
        }
        Ok(())
    }
}
