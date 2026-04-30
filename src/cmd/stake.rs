// stake — show on-chain stake status across both Ardi + KYA worknets.
//
// READS LIVE FROM CHAIN — not from the coordinator's API. The coordinator
// only knows about mints; eligibility is enforced on-chain at every
// commit() against AWPRegistry.getAgentInfo, so the truth lives there.

use anyhow::Result;
use alloy_primitives::{Address, U256};
use alloy_sol_types::SolCall;
use serde_json::json;
use std::str::FromStr;

use crate::auth::get_address;
use crate::chain::AWPRegistry;
use crate::output::{Internal, Output};
use crate::tx;

const ARDI_WORKNET_ID: &str = "845300000012";
const KYA_WORKNET_ID: &str = "845300000014";
const VE_AWP: &str = "0x0000b534C63D78212f1BDCc315165852793A00A8";
const AWP_ALLOCATOR: &str = "0x0000D6BB5e040E35081b3AaF59DD71b21C9800AA";
const AWP_REGISTRY: &str = "0x0000F34Ed3594F54faABbCb2Ec45738DDD1c001A";
const MIN_STAKE_AWP: u128 = 10_000;
const ONE_AWP_WEI: u128 = 1_000_000_000_000_000_000;

struct StakeOnWorknet {
    is_valid: bool,
    stake_awp: f64,
    stake_wei: U256,
}

fn read_stake(agent: Address, worknet_id_str: &str) -> Result<StakeOnWorknet> {
    let registry = Address::from_str(AWP_REGISTRY)?;
    let wn = U256::from_str_radix(worknet_id_str, 10)?;
    let call = AWPRegistry::getAgentInfoCall { agent, worknetId: wn };
    let raw = tx::view_call(&registry, call.abi_encode())?;
    let decoded = AWPRegistry::getAgentInfoCall::abi_decode_returns(&raw, true)?;
    let stake_wei = decoded.stake;
    let stake_awp = (stake_wei.to::<u128>() as f64) / (ONE_AWP_WEI as f64);
    Ok(StakeOnWorknet {
        is_valid: decoded.isValid,
        stake_awp,
        stake_wei,
    })
}

pub fn run(_server_url: &str) -> Result<()> {
    let address_str = get_address()?;
    let address = Address::from_str(&address_str)?;
    let min_stake_wei = U256::from(MIN_STAKE_AWP) * U256::from(ONE_AWP_WEI);

    let ardi = read_stake(address, ARDI_WORKNET_ID)?;
    let kya  = read_stake(address, KYA_WORKNET_ID)?;

    let ardi_ok = ardi.is_valid && ardi.stake_wei >= min_stake_wei;
    let kya_ok  = kya.is_valid  && kya.stake_wei  >= min_stake_wei;
    let eligible = ardi_ok || kya_ok;

    let path_taken = if ardi_ok { "ardi (self-stake)" }
                     else if kya_ok { "kya (delegated)" }
                     else { "neither" };

    let mut lines = vec![
        format!("Agent:                  {address_str}"),
        format!("Threshold (each path):  {} AWP", MIN_STAKE_AWP),
        format!(
            "Ardi worknet  ({}):  {:.2} AWP  →  {}",
            ARDI_WORKNET_ID,
            ardi.stake_awp,
            if ardi_ok { "✓ PASSES" } else { "✗ below threshold" }
        ),
        format!(
            "KYA worknet   ({}):  {:.2} AWP  →  {}",
            KYA_WORKNET_ID,
            kya.stake_awp,
            if kya_ok { "✓ PASSES" } else { "✗ below threshold" }
        ),
        format!(
            "Status:                 {}",
            if eligible { format!("✓ ELIGIBLE (via {path_taken})") } else { "✗ NOT ELIGIBLE".into() }
        ),
    ];

    if eligible {
        lines.push(String::new());
        lines.push("You're staked. Run `ardi-agent preflight` to confirm full readiness.".into());
        Output::success(
            lines.join("\n"),
            json!({
                "address": address_str,
                "eligible": true,
                "via": path_taken,
                "ardi_stake_awp": ardi.stake_awp,
                "kya_stake_awp": kya.stake_awp,
                "min_stake_awp": MIN_STAKE_AWP,
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

    // Below threshold on both. Tell the agent precisely which threshold
    // they failed and how much more is needed on whichever path is closer.
    let ardi_short = (MIN_STAKE_AWP as f64) - ardi.stake_awp;
    let kya_short  = (MIN_STAKE_AWP as f64) - kya.stake_awp;
    lines.push(String::new());
    lines.push("RULE: passing EITHER worknet's threshold makes you eligible.".into());
    lines.push(format!(
        "      Ardi shortfall: {ardi_short:.2} AWP   ·   KYA shortfall: {kya_short:.2} AWP"
    ));
    lines.push(String::new());
    lines.push("To become eligible — pick whichever path is shorter for you:".into());
    lines.push(String::new());
    lines.push("──── [A] Self-stake on Ardi worknet ────".into());
    lines.push("    https://awp.pro/staking".into());
    lines.push(format!(
        "    Connect your wallet, lock ≥{} AWP, and allocate to:", MIN_STAKE_AWP
    ));
    lines.push(format!("      agent: {address_str}"));
    lines.push(format!("      worknetId: {ARDI_WORKNET_ID}  (Ardi)"));
    lines.push(String::new());
    lines.push("──── [B] KYA delegated path (also accepted by Ardi — same threshold) ────".into());
    lines.push("    https://kya.link/".into());
    lines.push(format!(
        "    KYA verifies you on Twitter and sponsors stake into worknet {KYA_WORKNET_ID}."
    ));
    lines.push(format!(
        "    Sponsorship size depends on KYA's policy; if you only got partial"
    ));
    lines.push(format!(
        "    (e.g. 6,000 of {} required), you must still self-top-up the remainder", MIN_STAKE_AWP
    ));
    lines.push(format!(
        "    on the same KYA worknet ({KYA_WORKNET_ID}) OR fully self-stake on Ardi (path A)."
    ));
    lines.push(String::new());
    lines.push("──── [C] Programmatic — direct contract calls (advanced) ────".into());
    lines.push("    Base mainnet (chainId 8453):".into());
    lines.push(format!(
        "    1) veAWP.deposit(amount={}e18, lockDuration)", MIN_STAKE_AWP
    ));
    lines.push(format!("       contract: {VE_AWP}"));
    lines.push(format!(
        "    2) AWPAllocator.allocate(staker=you, agent={address_str},"
    ));
    lines.push(format!(
        "                              worknetId=<ARDI or KYA>, amount={}e18)", MIN_STAKE_AWP
    ));
    lines.push(format!("       contract: {AWP_ALLOCATOR}"));
    lines.push(String::new());
    lines.push("After staking, wait ~10s, then re-run: ardi-agent stake".into());

    Output::error_with_debug(
        lines.join("\n"),
        "NOT_STAKED",
        "stake",
        false,
        "Reach the 10,000 AWP threshold on EITHER Ardi (845300000012) OR KYA (845300000014) worknet, then re-run.",
        json!({
            "address": address_str,
            "eligible": false,
            "via": null,
            "ardi_worknet_id": ARDI_WORKNET_ID,
            "kya_worknet_id":  KYA_WORKNET_ID,
            "ardi_stake_awp":  ardi.stake_awp,
            "kya_stake_awp":   kya.stake_awp,
            "min_stake_awp":   MIN_STAKE_AWP,
            "ardi_shortfall_awp": ardi_short.max(0.0),
            "kya_shortfall_awp":  kya_short.max(0.0),
            "ardi_path_isvalid": ardi.is_valid,
            "kya_path_isvalid":  kya.is_valid,
            "ve_awp_contract":      VE_AWP,
            "awp_allocator_contract": AWP_ALLOCATOR,
            "awp_registry_contract": AWP_REGISTRY,
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
