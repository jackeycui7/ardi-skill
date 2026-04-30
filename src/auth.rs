// Resolve agent address. Three sources in priority order:
//   1. ARDI_AGENT_ADDR env (explicit override for testing)
//   2. AWP_ADDRESS env (set by awp-wallet unlock or upstream)
//   3. awp-wallet receive (shell out)

use anyhow::{anyhow, Result};

use crate::wallet::WalletStatus;

pub fn get_address() -> Result<String> {
    if let Ok(a) = std::env::var("ARDI_AGENT_ADDR") {
        if !a.is_empty() {
            return Ok(normalize_addr(&a)?);
        }
    }
    if let Ok(a) = std::env::var("AWP_ADDRESS") {
        if !a.is_empty() {
            return Ok(normalize_addr(&a)?);
        }
    }
    let s = WalletStatus::check();
    s.address
        .ok_or_else(|| anyhow!("could not determine agent address: {}", s.human_status))
}

fn normalize_addr(a: &str) -> Result<String> {
    let s = a.trim();
    if !s.starts_with("0x") || s.len() != 42 {
        return Err(anyhow!("invalid address: {s}"));
    }
    Ok(s.to_lowercase())
}
