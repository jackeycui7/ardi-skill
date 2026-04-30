// Wallet — sign + broadcast EIP-1559 txs entirely in this process.
//
// Single key source: awp-wallet export-private-key. We shell out, get
// the key, hold it in memory just long enough to build a signer, sign,
// drop. Key is never persisted, logged, or written to disk by us.
//
// Signing is done with alloy_signer_local::PrivateKeySigner against an
// EIP-1559 tx envelope, then broadcast via eth_sendRawTransaction through
// our RPC pool.
//
// We deliberately do NOT support reading a raw key from env. End users
// should use awp-wallet — the standard AWP wallet skill — and not be
// invited to paste private keys into shell config.

use anyhow::{anyhow, Context, Result};
use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_network::TxSignerSync;
use alloy_primitives::{Address, Bytes, TxKind, U256};
use alloy_rlp::Encodable;
use alloy_signer_local::PrivateKeySigner;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

use crate::log_debug;
use crate::rpc;

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

        if let Ok(path) = which("awp-wallet") {
            s.cli_installed = true;
            s.cli_path = Some(path.clone());
            let dir = wallet_dir();
            s.wallet_dir_exists = dir.exists();
            s.has_keystore =
                dir.join("keystore.json").exists() || dir.join("wallet.json").exists();

            if let Ok(out) = Command::new(&path).arg("receive").output() {
                if out.status.success() {
                    let txt = String::from_utf8_lossy(&out.stdout);
                    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                        // awp-wallet v1.4 returns {"eoaAddress":"0x..."}.
                        // Older versions returned {"address":"0x..."}; honor both.
                        if let Some(addr) = v
                            .get("eoaAddress")
                            .or_else(|| v.get("address"))
                            .and_then(|x| x.as_str())
                        {
                            s.can_receive = true;
                            s.address = Some(addr.to_lowercase());
                        }
                    }
                }
            }
        }

        s.human_status = match (s.cli_installed, s.has_keystore, s.can_receive) {
            (false, _, _) => "awp-wallet not installed".into(),
            (true, false, _) => "awp-wallet installed but no wallet — run `awp-wallet setup`".into(),
            (true, true, true) => {
                format!("awp-wallet ready; agent = {}", s.address.as_deref().unwrap_or("?"))
            }
            (true, true, false) => {
                "wallet exists but inaccessible (do NOT re-run setup — overwrites)".into()
            }
        };
        s
    }

    pub fn safe_to_init(&self) -> bool {
        !self.has_keystore
    }

    pub fn setup_command(&self) -> &'static str {
        if !self.cli_installed {
            "git clone https://github.com/awp-core/awp-wallet ~/awp-wallet && cd ~/awp-wallet && bash install.sh && awp-wallet setup"
        } else if self.safe_to_init() {
            "awp-wallet setup"
        } else {
            "(wallet already exists — do not re-init)"
        }
    }

    pub fn suggestion(&self) -> String {
        if !self.cli_installed {
            "Install awp-wallet first: `git clone https://github.com/awp-core/awp-wallet \
             ~/awp-wallet && cd ~/awp-wallet && bash install.sh && awp-wallet setup`."
                .into()
        } else if !self.has_keystore {
            "Run `awp-wallet setup` to create your wallet.".into()
        } else if !self.can_receive {
            "Wallet exists but unreadable. DO NOT run setup — that overwrites. \
             Investigate awp-wallet install."
                .into()
        } else {
            "Wallet OK.".into()
        }
    }
}

fn wallet_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".awp-wallet")
}

fn which(bin: &str) -> Result<PathBuf> {
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

/// Resolve the private key for signing. Caller is responsible for not
/// persisting / logging the returned string.
fn resolve_private_key() -> Result<String> {
    let bin = which("awp-wallet").context(
        "awp-wallet not installed. Install via: \
         `git clone https://github.com/awp-core/awp-wallet ~/awp-wallet && \
          cd ~/awp-wallet && bash install.sh && awp-wallet setup`",
    )?;
    log_debug!("resolve_private_key: shelling out to awp-wallet export-private-key");
    // awp-wallet v1.4 returns {"privateKey":"0x...","address":"0x...","warning":"..."}
    // immediately, no confirm prompt — wallet is unlocked-by-default in v1.x.
    let out = Command::new(&bin)
        .arg("export-private-key")
        .output()
        .context("failed to invoke awp-wallet export-private-key")?;
    if !out.status.success() {
        return Err(anyhow!(
            "awp-wallet export-private-key failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    // awp-wallet may emit either {"privateKey":"0x..."} or a plain 0x... line.
    if let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) {
        if let Some(pk) = v
            .get("privateKey")
            .or_else(|| v.get("private_key"))
            .and_then(|x| x.as_str())
        {
            return Ok(pk.to_string());
        }
    }
    let trimmed = stdout.trim().to_string();
    if trimmed.starts_with("0x") && trimmed.len() == 66 {
        return Ok(trimmed);
    }
    Err(anyhow!(
        "awp-wallet export-private-key returned unexpected output (expected JSON {{privateKey:0x...}} or bare 0x... line)"
    ))
}

/// Sign + broadcast an EIP-1559 transaction. The `tx` JSON is the same
/// shape `tx::build_tx` produces. Returns the broadcast tx hash.
pub fn send_tx(tx: &Value) -> Result<String> {
    // Pull required fields from the tx JSON.
    let to = tx
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("tx missing `to`"))?;
    let data_hex = tx
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("tx missing `data`"))?;
    let value_hex = tx
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("0x0");
    let nonce = tx
        .get("nonce")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("tx missing `nonce`"))?;
    let gas_hex = tx
        .get("gas")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("tx missing `gas`"))?;
    let max_fee_hex = tx
        .get("maxFeePerGas")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("tx missing `maxFeePerGas`"))?;
    let max_prio_hex = tx
        .get("maxPriorityFeePerGas")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("tx missing `maxPriorityFeePerGas`"))?;
    let chain_id = tx
        .get("chainId")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("tx missing `chainId`"))?;

    let to_addr: Address = to
        .parse()
        .map_err(|e| anyhow!("invalid `to` address: {e}"))?;
    let value_u256 = U256::from_str_radix(value_hex.trim_start_matches("0x"), 16)
        .map_err(|e| anyhow!("invalid `value` hex: {e}"))?;
    let gas = u64::from_str_radix(gas_hex.trim_start_matches("0x"), 16)
        .map_err(|e| anyhow!("invalid `gas`: {e}"))?;
    let max_fee = u128::from_str_radix(max_fee_hex.trim_start_matches("0x"), 16)
        .map_err(|e| anyhow!("invalid `maxFeePerGas`: {e}"))?;
    let max_prio = u128::from_str_radix(max_prio_hex.trim_start_matches("0x"), 16)
        .map_err(|e| anyhow!("invalid `maxPriorityFeePerGas`: {e}"))?;
    let data_bytes = hex::decode(data_hex.trim_start_matches("0x"))
        .map_err(|e| anyhow!("invalid `data` hex: {e}"))?;

    // Resolve key + immediately use to build a signer; don't keep the hex
    // string around longer than necessary.
    let pk = resolve_private_key()?;
    let signer: PrivateKeySigner = pk
        .parse()
        .map_err(|e| anyhow!("private key parse failed: {e}"))?;
    drop(pk); // best-effort scrub

    let mut unsigned = TxEip1559 {
        chain_id,
        nonce,
        gas_limit: gas,
        max_fee_per_gas: max_fee,
        max_priority_fee_per_gas: max_prio,
        to: TxKind::Call(to_addr),
        value: value_u256,
        access_list: Default::default(),
        input: Bytes::from(data_bytes),
    };

    let signature = signer
        .sign_transaction_sync(&mut unsigned)
        .map_err(|e| anyhow!("EIP-1559 sign failed: {e}"))?;
    let signed = unsigned.into_signed(signature);

    // RLP-encode envelope as 0x02 || rlp(signed).
    let mut raw = Vec::with_capacity(256);
    raw.push(0x02);
    signed.rlp_encode(&mut raw);

    // Broadcast.
    let raw_hex = format!("0x{}", hex::encode(&raw));
    let r = rpc::call("eth_sendRawTransaction", json!([raw_hex]))?;
    let hash = r
        .as_str()
        .ok_or_else(|| anyhow!("eth_sendRawTransaction returned non-string: {r}"))?
        .to_string();
    Ok(hash)
}

/// EIP-712 typed data signing — used by AWP gasless registration.
/// Same dual key path. Computes the EIP-712 digest via alloy_dyn_abi
/// then sign-hashes with the local signer (sync, no tokio).
pub fn sign_typed_data(typed_data_json: &Value) -> Result<String> {
    use alloy_dyn_abi::TypedData;
    use alloy_signer::SignerSync;

    let pk = resolve_private_key()?;
    let signer: PrivateKeySigner = pk
        .parse()
        .map_err(|e| anyhow!("private key parse failed: {e}"))?;
    drop(pk);

    let typed: TypedData = serde_json::from_value(typed_data_json.clone())
        .map_err(|e| anyhow!("typed data parse failed: {e}"))?;
    let digest = typed
        .eip712_signing_hash()
        .map_err(|e| anyhow!("EIP-712 digest compute failed: {e}"))?;
    let signature = signer
        .sign_hash_sync(&digest)
        .map_err(|e| anyhow!("EIP-712 hash sign failed: {e}"))?;

    Ok(format!("0x{}", hex::encode(signature.as_bytes())))
}
