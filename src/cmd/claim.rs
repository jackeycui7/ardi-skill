// claim — pull accumulated $ardi rewards from the EmissionDistributor.
//
// v3: every active NFT accrues emission via accPerShare. The holder's pending
// balance includes (a) anything settled by prior transfer/deactivate hooks,
// plus (b) the current rolling accrual on each token they pass in. Empty
// list is fine — claims just the settled balance.

use anyhow::{anyhow, Context, Result};
use alloy_primitives::{Address, U256};
use alloy_sol_types::SolCall;
use serde_json::json;
use std::str::FromStr;

use crate::auth::get_address;
use crate::chain::EmissionDistributor;
use crate::client::ApiClient;
use crate::output::{Internal, Output};
use crate::tx;
use crate::log_info;

pub fn run(server_url: &str, token_ids: Vec<u64>) -> Result<()> {
    let agent_str = get_address()?;
    let agent = Address::from_str(&agent_str)?;
    let api = ApiClient::new(server_url)?;

    let cfg: serde_json::Value = api
        .get_json("/v1/chain/contracts")
        .or_else(|_| api.get_json("/v1/health"))
        .unwrap_or_default();
    let dist_addr = read_addr(&cfg, "emission_distributor")
        .or_else(|| std::env::var("EMISSION_DISTRIBUTOR_ADDR").ok())
        .ok_or_else(|| {
            anyhow!(
                "server didn't return emission_distributor; set EMISSION_DISTRIBUTOR_ADDR env"
            )
        })?;
    let dist_addr = Address::from_str(&dist_addr)?;

    let token_ids_u: Vec<U256> = token_ids.iter().map(|&t| U256::from(t)).collect();

    // Pre-flight: print pending so the agent knows what they're getting.
    let pending_raw = tx::view_call(
        &dist_addr,
        EmissionDistributor::pendingForCall {
            holder: agent,
            tokenIds: token_ids_u.clone(),
        }
        .abi_encode(),
    )?;
    let pending = EmissionDistributor::pendingForCall::abi_decode_returns(&pending_raw, true)?._0;
    log_info!("claim: pending={pending} ardi over {} tokens", token_ids.len());

    if pending == U256::ZERO {
        Output::success(
            "Nothing to claim - pending balance is zero.".to_string(),
            json!({ "pending_wei": "0" }),
            Internal {
                next_action: "skip".into(),
                next_command: None,
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    let data = tx::calldata_claim(token_ids_u);
    let tx_obj = tx::build_tx(&agent, &dist_addr, data, 0, 200_000)?;
    let claim_hash = tx::send_and_wait(&tx_obj).context("send claim tx")?;

    Output::success(
        format!("Claimed {pending} ardi (tx {claim_hash})."),
        json!({
            "claim_tx": claim_hash,
            "amount_wei": pending.to_string(),
            "tokens": token_ids,
        }),
        Internal {
            next_action: "done".into(),
            next_command: None,
            ..Default::default()
        },
    )
    .print();
    Ok(())
}

fn read_addr(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .or_else(|| v.get("contracts").and_then(|c| c.get(key)))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}
