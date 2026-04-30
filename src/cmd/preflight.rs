// preflight — full env check before any chain-touching command.
//
// Order:
//   1/5  resolve agent address (awp-wallet or env)
//   2/5  AWP network registration (auto-register if needed, gasless)
//   3/5  coordinator reachable
//   4/5  Base ETH balance ≥ minimum
//   5/5  stake eligible on Ardi worknet

use anyhow::Result;
use serde_json::json;

use crate::auth::get_address;
use crate::awp_register;
use crate::client::ApiClient;
use crate::output::{Internal, Output};
use crate::wallet::WalletStatus;
use crate::{cmd, log_error, log_info};

pub fn run(server_url: &str) -> Result<()> {
    log_info!("preflight: starting (server={server_url})");

    // 1/5 wallet
    log_info!("preflight [1/5]: resolving agent address...");
    let address = match get_address() {
        Ok(a) => {
            log_info!("preflight [1/5]: address = {a}");
            a
        }
        Err(e) => {
            let s = WalletStatus::check();
            Output::error_with_debug(
                format!("Cannot determine agent address: {e}"),
                "WALLET_NOT_CONFIGURED",
                "dependency",
                false,
                s.suggestion(),
                json!({
                    "wallet": {
                        "cli_installed": s.cli_installed,
                        "wallet_dir_exists": s.wallet_dir_exists,
                        "has_keystore": s.has_keystore,
                        "can_receive": s.can_receive,
                        "safe_to_init": s.safe_to_init(),
                    },
                }),
                Internal {
                    next_action: "configure_wallet".into(),
                    next_command: Some(s.setup_command().into()),
                    progress: Some("0/5".into()),
                },
            )
            .print();
            return Ok(());
        }
    };

    // 2/5 AWP registration (gasless)
    log_info!("preflight [2/5]: checking AWP network registration...");
    let reg = match awp_register::ensure_registered(&address) {
        Ok(r) => r,
        Err(e) => {
            log_error!("preflight [2/5]: {e:#}");
            Output::error(
                format!("AWP registration step failed: {e}"),
                "AWP_REGISTER_FAILED",
                "network",
                true,
                "Check internet connectivity to api.awp.sh, then re-run preflight.",
                Internal {
                    next_action: "retry".into(),
                    next_command: Some("ardi-agent preflight".into()),
                    progress: Some("1/5".into()),
                },
            )
            .print();
            return Ok(());
        }
    };
    if !reg.registered {
        Output::error(
            reg.message,
            "AWP_NOT_REGISTERED",
            "registration",
            true,
            "Wait 30s and re-run preflight.",
            Internal {
                next_action: "wait_and_retry".into(),
                next_command: Some("ardi-agent preflight".into()),
                progress: Some("1/5".into()),
            },
        )
        .print();
        return Ok(());
    }
    log_info!("preflight [2/5]: registered (auto={})", reg.auto_registered);

    // 3/5 coordinator
    log_info!("preflight [3/5]: pinging coordinator {server_url}...");
    let api = ApiClient::new(server_url)?;
    if let Err(e) = api.ping() {
        Output::error(
            format!("Coordinator unreachable: {e}"),
            "COORDINATOR_UNREACHABLE",
            "network",
            true,
            "Check ARDI_COORDINATOR_URL and your network. Default: https://api.ardinals.com",
            Internal {
                next_action: "retry".into(),
                next_command: Some("ardi-agent preflight".into()),
                progress: Some("2/5".into()),
            },
        )
        .print();
        return Ok(());
    }
    log_info!("preflight [3/5]: coordinator OK");

    // 4/5 gas
    log_info!("preflight [4/5]: checking Base ETH balance...");
    let gas_check = cmd::gas::check_balance(&address)?;
    if !gas_check.sufficient {
        Output::error(
            format!(
                "Wallet has {:.6} ETH on Base — below the {:.6} ETH safety floor.",
                gas_check.balance_eth, gas_check.min_eth
            ),
            "INSUFFICIENT_GAS",
            "balance",
            true,
            format!(
                "Send at least {:.4} ETH to {address} on Base mainnet, then re-run preflight.",
                gas_check.recommended_eth
            ),
            Internal {
                next_action: "fund_gas".into(),
                next_command: Some("ardi-agent preflight".into()),
                progress: Some("3/5".into()),
            },
        )
        .print();
        return Ok(());
    }
    log_info!(
        "preflight [4/5]: gas OK ({:.6} ETH)",
        gas_check.balance_eth
    );

    // 5/5 stake
    log_info!("preflight [5/5]: checking stake eligibility...");
    let stake_state: Option<serde_json::Value> =
        api.try_get_json(&format!("/v1/agent/{address}/state"))?;
    let eligible = stake_state
        .as_ref()
        .and_then(|v| v.get("eligible"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !eligible {
        Output::error(
            "Agent not yet staked / eligible on Ardi WorkNet.",
            "NOT_STAKED",
            "stake",
            false,
            "Run `ardi-agent stake` to see the 3 paths to become eligible.",
            Internal {
                next_action: "guide_stake".into(),
                next_command: Some("ardi-agent stake".into()),
                progress: Some("4/5".into()),
            },
        )
        .print();
        return Ok(());
    }

    Output::success(
        format!("Preflight passed. Agent {address} is ready to mine."),
        json!({
            "address": address,
            "registered": reg.registered,
            "balance_eth": gas_check.balance_eth,
            "stake_eligible": eligible,
        }),
        Internal {
            next_action: "ready".into(),
            next_command: Some("ardi-agent mine".into()),
            progress: Some("5/5".into()),
        },
    )
    .print();
    Ok(())
}
