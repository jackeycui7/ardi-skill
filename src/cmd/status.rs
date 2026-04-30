// status — combined view: address, AWP registration, ETH balance, stake, recent activity.

use anyhow::Result;
use serde_json::json;

use crate::auth::get_address;
use crate::awp_register;
use crate::client::ApiClient;
use crate::output::{Internal, Output};
use crate::{cmd, log_info};

pub fn run(server_url: &str) -> Result<()> {
    let address = get_address()?;
    log_info!("status: address = {address}");

    let registered = awp_register::check_registration(&address).unwrap_or(false);
    let gas = cmd::gas::check_balance(&address)?;

    let api = ApiClient::new(server_url)?;
    let coord_health = api.ping().ok();
    let agent_state: Option<serde_json::Value> = api
        .try_get_json(&format!("/v1/agent/{address}/state"))
        .unwrap_or(None);

    let summary = vec![
        format!("Agent address     : {address}"),
        format!(
            "AWP registered    : {}",
            if registered { "yes" } else { "no" }
        ),
        format!("Base ETH balance  : {:.6} ETH", gas.balance_eth),
        format!(
            "Coordinator       : {}",
            if coord_health.is_some() { "reachable" } else { "UNREACHABLE" }
        ),
        format!(
            "Stake state       : {}",
            agent_state
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "(no state)".into())
        ),
    ];

    Output::success(
        summary.join("\n"),
        json!({
            "address": address,
            "registered": registered,
            "balance_eth": gas.balance_eth,
            "coord_reachable": coord_health.is_some(),
            "agent_state": agent_state,
        }),
        Internal {
            next_action: "review".into(),
            next_command: Some("ardi-agent preflight".into()),
            ..Default::default()
        },
    )
    .print();

    Ok(())
}
