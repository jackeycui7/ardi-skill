// awp-wallet bridge — the skill never holds private keys. All signing
// shells out to the awp-wallet CLI. This module checks if the wallet is
// installed, returns its address, and provides typed-data / tx signing
// wrappers that other modules call.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Default, Clone)]
pub struct WalletStatus {
    pub cli_installed: bool,
    pub cli_path: Option<PathBuf>,
    pub wallet_dir_exists: bool,
    pub has_keystore: bool,
    pub can_receive: bool,
    pub address: Option<String>,
    pub human_status: String,
}

impl WalletStatus {
    pub fn check() -> Self {
        let mut s = Self::default();

        // Find awp-wallet binary on PATH.
        if let Ok(path) = which("awp-wallet") {
            s.cli_installed = true;
            s.cli_path = Some(path);
        }

        // Check ~/.awp-wallet directory.
        let dir = wallet_dir();
        s.wallet_dir_exists = dir.exists();
        s.has_keystore = dir.join("keystore.json").exists() || dir.join("wallet.json").exists();

        // Try `awp-wallet receive` to confirm wallet is accessible.
        if s.cli_installed {
            if let Some(p) = &s.cli_path {
                if let Ok(out) = Command::new(p).arg("receive").output() {
                    if out.status.success() {
                        let txt = String::from_utf8_lossy(&out.stdout);
                        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                            if let Some(addr) = v.get("address").and_then(|x| x.as_str()) {
                                s.can_receive = true;
                                s.address = Some(addr.to_string());
                            }
                        }
                    }
                }
            }
        }

        s.human_status = match (s.cli_installed, s.has_keystore, s.can_receive) {
            (false, _, _) => "awp-wallet not installed".into(),
            (true, false, _) => "awp-wallet installed but no wallet — safe to run `awp-wallet setup`".into(),
            (true, true, true) => format!("ready: {}", s.address.as_deref().unwrap_or("?")),
            (true, true, false) => "wallet exists but inaccessible (do NOT run init — would overwrite)".into(),
        };

        s
    }

    /// Is it safe to run `awp-wallet setup` / `init`? Only when no wallet exists.
    pub fn safe_to_init(&self) -> bool {
        !self.has_keystore
    }

    pub fn setup_command(&self) -> &'static str {
        if !self.cli_installed {
            "git clone https://github.com/awp-core/awp-wallet.git ~/awp-wallet && cd ~/awp-wallet && bash install.sh"
        } else if self.safe_to_init() {
            "awp-wallet setup"
        } else {
            "(wallet already exists — do not re-init)"
        }
    }

    pub fn suggestion(&self) -> String {
        if !self.cli_installed {
            "Install awp-wallet first. See setup_command in debug output."
        } else if !self.has_keystore {
            "Run `awp-wallet setup` to create your agent wallet."
        } else if !self.can_receive {
            "Wallet exists but unreadable. DO NOT run setup — that overwrites. Investigate awp-wallet install."
        } else {
            "Wallet OK."
        }
        .to_string()
    }
}

fn wallet_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".awp-wallet")
}

/// Sign EIP-712 typed data via awp-wallet bridge. Returns 0x-prefixed signature.
pub fn sign_typed_data(typed_data_json: &Value) -> Result<String> {
    let bin = which("awp-wallet").context("awp-wallet not installed")?;
    let payload = serde_json::to_string(typed_data_json)?;
    let out = Command::new(&bin)
        .arg("sign-typed-data")
        .arg("--data")
        .arg(&payload)
        .output()
        .context("failed to invoke awp-wallet sign-typed-data")?;
    if !out.status.success() {
        return Err(anyhow!(
            "awp-wallet sign-typed-data failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    v.get("signature")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow!("no signature field in awp-wallet output"))
}

/// Send raw EIP-1559 tx via awp-wallet bridge. `tx` is a JSON object
/// with `to`, `data`, `value`, `chainId`, `gas`, `maxFeePerGas`, etc.
/// Returns the tx hash.
pub fn send_tx(tx: &Value) -> Result<String> {
    let bin = which("awp-wallet").context("awp-wallet not installed")?;
    let payload = serde_json::to_string(tx)?;
    let out = Command::new(&bin)
        .arg("send-tx")
        .arg("--tx")
        .arg(&payload)
        .output()
        .context("failed to invoke awp-wallet send-tx")?;
    if !out.status.success() {
        return Err(anyhow!(
            "awp-wallet send-tx failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    v.get("hash")
        .or_else(|| v.get("txHash"))
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow!("no tx hash in awp-wallet output"))
}

fn which(bin: &str) -> Result<PathBuf> {
    // Honor explicit override.
    if let Ok(p) = std::env::var("AWP_WALLET_BIN") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Ok(pb);
        }
    }
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        let candidate = PathBuf::from(dir).join(bin);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!("`{bin}` not found on PATH"))
}
