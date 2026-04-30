// stake — show eligibility + 3-path guidance.
//
// Path A: self-stake via awp.pro UI
// Path B: KYA delegated staking (KYA verifies via Twitter, sponsors 10K AWP)
// Path C: programmatic — direct contract calls

use anyhow::Result;
use serde_json::json;

use crate::auth::get_address;
use crate::client::ApiClient;
use crate::output::{Internal, Output};

const ARDI_WORKNET_ID: &str = "845300000012";
const KYA_WORKNET_ID: &str = "845300000014";
const VE_AWP: &str = "0x0000b534C63D78212f1BDCc315165852793A00A8";
const AWP_ALLOCATOR: &str = "0x0000D6BB5e040E35081b3AaF59DD71b21C9800AA";
const MIN_STAKE_AWP: &str = "10000";

pub fn run(server_url: &str) -> Result<()> {
    let address = get_address()?;
    let api = ApiClient::new(server_url)?;
    let state: Option<serde_json::Value> =
        api.try_get_json(&format!("/v1/agent/{address}/state"))?;
    let eligible = state
        .as_ref()
        .and_then(|v| v.get("eligible"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let stake_amount = state
        .as_ref()
        .and_then(|v| v.get("stake_awp"))
        .and_then(|v| v.as_str())
        .unwrap_or("0")
        .to_string();

    let mut lines = vec![
        format!("Agent:           {address}"),
        format!("Current stake:   {stake_amount} AWP"),
        format!("Required:        {MIN_STAKE_AWP} AWP on Ardi worknet (id {ARDI_WORKNET_ID})"),
        format!(
            "Status:          {}",
            if eligible { "✓ ELIGIBLE" } else { "✗ NOT ELIGIBLE" }
        ),
    ];

    if eligible {
        lines.push(String::new());
        lines.push("You're staked. Run `ardi-agent preflight` to confirm full readiness.".into());
        Output::success(
            lines.join("\n"),
            json!({
                "address": address,
                "eligible": true,
                "ardi_worknet_id": ARDI_WORKNET_ID,
            }),
            Internal {
                next_action: "ready".into(),
                next_command: Some("ardi-agent preflight".into()),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    // 3-path guidance
    lines.push(String::new());
    lines.push("To become eligible — pick whichever path fits you:".into());
    lines.push(String::new());
    lines.push("──── [A] Easiest — self-stake via official AWP web UI ────".into());
    lines.push("    https://awp.pro/staking".into());
    lines.push(format!(
        "    Connect your wallet, lock ≥{MIN_STAKE_AWP} AWP, and allocate to:"
    ));
    lines.push(format!(
        "      agent: {address}"
    ));
    lines.push(format!(
        "      worknetId: {ARDI_WORKNET_ID}  (Ardi mainnet)"
    ));
    lines.push("    The UI walks you through every step.".into());
    lines.push(String::new());
    lines.push("──── [B] No-AWP path — KYA delegated staking ────".into());
    lines.push("    https://kya.link/".into());
    lines.push("    KYA verifies you (Twitter post) then sponsors stake on your behalf:".into());
    lines.push(format!(
        "      agent: {address}"
    ));
    lines.push(format!(
        "      worknetId: {KYA_WORKNET_ID}  (KYA delegated path, also accepted by Ardi)"
    ));
    lines.push("    You don't need to hold AWP yourself.".into());
    lines.push(String::new());
    lines.push("──── [C] Programmatic — direct contract calls (advanced) ────".into());
    lines.push("    Base mainnet (chainId 8453):".into());
    lines.push(format!(
        "    1) veAWP.deposit(amount={MIN_STAKE_AWP}e18, lockDuration)"
    ));
    lines.push(format!("       contract: {VE_AWP}"));
    lines.push(format!(
        "    2) AWPAllocator.allocate(staker=you, agent={address},"
    ));
    lines.push(format!(
        "                              worknetId={ARDI_WORKNET_ID}, amount={MIN_STAKE_AWP}e18)"
    ));
    lines.push(format!("       contract: {AWP_ALLOCATOR}"));
    lines.push(String::new());
    lines.push("After staking, wait ~10s, then re-run: ardi-agent stake".into());

    Output::error_with_debug(
        lines.join("\n"),
        "NOT_STAKED",
        "stake",
        false,
        "Complete one of the three paths above, then re-run `ardi-agent stake`.",
        json!({
            "address": address,
            "ardi_worknet_id": ARDI_WORKNET_ID,
            "kya_worknet_id": KYA_WORKNET_ID,
            "min_stake_awp": MIN_STAKE_AWP,
            "ve_awp_contract": VE_AWP,
            "awp_allocator_contract": AWP_ALLOCATOR,
        }),
        Internal {
            next_action: "stake_required".into(),
            next_command: Some("ardi-agent stake".into()),
            ..Default::default()
        },
    )
    .print();

    Ok(())
}
