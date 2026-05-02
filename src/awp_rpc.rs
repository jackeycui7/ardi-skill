// awp_rpc — minimal JSON-RPC 2.0 client for AWP rootnet API (api.awp.sh/v2).
//
// Used to discover which staker(s) are backing an agent, replacing the local
// indexer that we used to run on coord-rs. AWP indexes Allocated events
// across all chains and exposes them via `staking.getAllocationsByAgentSubnet`.
//
// The chain-side AWPAllocator.getAgentStake call remains the source of truth
// at commit time — this RPC is only for *discovery* (which staker address to
// pass to ArdiEpochDraw.commit). If AWP is down or returns stale data, the
// commit just reverts at the chain check; no funds at risk.

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const DEFAULT_AWP_RPC: &str = "https://api.awp.sh/v2";

pub struct AwpRpc {
    url: String,
    http: Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllocationRow {
    pub chain_id: i64,
    pub user_address: String, // = the staker
    pub amount: String,       // wei, decimal string
    pub frozen: bool,
}

impl AwpRpc {
    pub fn new() -> Result<Self> {
        let url = std::env::var("AWP_RPC_URL").unwrap_or_else(|_| DEFAULT_AWP_RPC.to_string());
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("ardi-agent/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { url, http })
    }

    fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });
        let resp = self
            .http
            .post(&self.url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .with_context(|| format!("POST {} method={method}", self.url))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {} from awp rpc: {text}", status.as_u16()));
        }
        let v: Value = serde_json::from_str(&text)
            .with_context(|| format!("parse awp rpc response: {text}"))?;
        if let Some(err) = v.get("error") {
            return Err(anyhow!("awp rpc error: {err}"));
        }
        v.get("result")
            .cloned()
            .ok_or_else(|| anyhow!("awp rpc missing result: {text}"))
    }

    /// Returns the list of (staker, amount) rows backing an agent in a worknet.
    /// Order is amount-desc per the spec. Cross-chain by default; pass `chain_id`
    /// > 0 to restrict to one chain.
    pub fn allocations_by_agent_worknet(
        &self,
        agent: &str,
        worknet_id: &str,
        chain_id: Option<i64>,
    ) -> Result<Vec<AllocationRow>> {
        let mut params = json!({
            "agent": agent,
            "worknetId": worknet_id,
        });
        if let Some(c) = chain_id {
            params["chainId"] = json!(c);
        }
        let v = self.call("staking.getAllocationsByAgentSubnet", params)?;
        let rows: Vec<AllocationRow> = serde_json::from_value(v)
            .context("decode allocation rows")?;
        Ok(rows)
    }
}
