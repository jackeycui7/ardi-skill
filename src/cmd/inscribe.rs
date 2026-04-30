// inscribe — check on-chain winners[ep][wid]; if it's us, mint via ArdiNFT.

use anyhow::{anyhow, Context, Result};
use alloy_primitives::{Address, U256};
use alloy_sol_types::SolCall;
use serde_json::json;
use std::str::FromStr;

use crate::auth::get_address;
use crate::chain::{ArdiEpochDraw, ArdiNFT};
use crate::client::ApiClient;
use crate::output::{Internal, Output};
use crate::state::{CommitStatus, State};
use crate::tx;

pub fn run(server_url: &str, epoch_id: u64, word_id: u64) -> Result<()> {
    let agent_str = get_address()?;
    let agent = Address::from_str(&agent_str)?;

    let mut st = State::load()?;
    let entry = match st.get(epoch_id, word_id).cloned() {
        Some(e) => e,
        None => {
            Output::error(
                format!("No local state for epoch={epoch_id} word_id={word_id}."),
                "NO_LOCAL_COMMIT",
                "state",
                false,
                "Inscribe requires a prior commit + reveal from THIS machine. \
                 If you committed elsewhere, the state file is on that machine.",
                Internal {
                    next_action: "list_state".into(),
                    next_command: Some("ardi-agent commits".into()),
                    ..Default::default()
                },
            )
            .print();
            return Ok(());
        }
    };
    if entry.status == CommitStatus::Inscribed {
        Output::success(
            format!("Already inscribed (token_id={:?}, tx={:?}).", entry.token_id, entry.inscribe_tx),
            json!({ "epoch_id": epoch_id, "word_id": word_id, "status": "inscribed" }),
            Internal {
                next_action: "done".into(),
                next_command: Some("ardi-agent commits".into()),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }
    if matches!(entry.status, CommitStatus::Lost) {
        Output::success(
            "Already marked lost (you weren't picked as winner). Nothing to inscribe.".to_string(),
            json!({ "epoch_id": epoch_id, "word_id": word_id, "status": "lost" }),
            Internal {
                next_action: "skip".into(),
                next_command: None,
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }
    if matches!(entry.status, CommitStatus::Committed) {
        Output::error(
            "Cannot inscribe before reveal. Did you skip `reveal`?".to_string(),
            "REVEAL_FIRST",
            "state",
            false,
            format!("Run: ardi-agent reveal --epoch {epoch_id} --word-id {word_id}"),
            Internal {
                next_action: "reveal_first".into(),
                next_command: Some(format!(
                    "ardi-agent reveal --epoch {epoch_id} --word-id {word_id}"
                )),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    let api = ApiClient::new(server_url)?;
    let ep: serde_json::Value = api.get_json(&format!("/v1/epoch/{epoch_id}"))?;
    let draw_addr = Address::from_str(
        ep.get("epoch_draw_contract")
            .and_then(|v| v.as_str())
            .or_else(|| ep.get("contract").and_then(|v| v.as_str()))
            .unwrap_or("0x0"),
    )?;
    let nft_addr = Address::from_str(
        ep.get("ardi_nft_contract")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("server didn't return ardi_nft_contract"))?,
    )?;

    // 1. Check on-chain winner.
    let call = ArdiEpochDraw::winnersCall {
        epochId: U256::from(epoch_id),
        wordId: U256::from(word_id),
    };
    let raw = tx::view_call(&draw_addr, call.abi_encode())?;
    if raw.len() < 32 {
        return Err(anyhow!("winners() returned <32 bytes"));
    }
    let winner = Address::from_slice(&raw[12..32]);

    if winner == Address::ZERO {
        // VRF hasn't fired yet, or no candidates.
        Output::success(
            "Winner not yet picked (VRF still pending or no correct revealers). Try again in 30s.",
            json!({ "epoch_id": epoch_id, "word_id": word_id, "winner": "0x0" }),
            Internal {
                next_action: "wait_vrf".into(),
                next_command: Some(format!(
                    "ardi-agent inscribe --epoch {epoch_id} --word-id {word_id}"
                )),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }
    if winner != agent {
        if let Some(e) = st.get_mut(epoch_id, word_id) {
            e.status = CommitStatus::Lost;
        }
        st.save()?;
        Output::success(
            format!("Better luck next time — winner is {winner:?}, not us."),
            json!({
                "epoch_id": epoch_id,
                "word_id": word_id,
                "winner": format!("0x{:x}", winner),
                "self": agent_str,
            }),
            Internal {
                next_action: "skip".into(),
                next_command: None,
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    // 2. We won — call inscribe.
    let salt_bytes = hex::decode(entry.salt_hex.trim_start_matches("0x"))?;
    let salt = alloy_primitives::B256::from_slice(&salt_bytes);
    let data = tx::calldata_inscribe(
        epoch_id,
        word_id,
        entry.answer.clone(),
        salt,
        entry.power,
        entry.language_id,
    );
    let tx_obj = tx::build_tx(&agent, &nft_addr, data, 0, 350_000)?;
    let tx_hash = tx::send_and_wait(&tx_obj).context("send inscribe tx")?;

    if let Some(e) = st.get_mut(epoch_id, word_id) {
        e.status = CommitStatus::Inscribed;
        e.inscribe_tx = Some(tx_hash.clone());
        // token_id determinable from receipt logs; we leave it None for now,
        // the `nft` command can backfill by reading totalInscribed sequence.
    }
    st.save()?;

    // Try totalInscribed to infer token_id (simpler than parsing logs).
    let ti = tx::view_call(
        &nft_addr,
        ArdiNFT::totalInscribedCall {}.abi_encode(),
    )?;
    let total = if ti.len() >= 32 {
        U256::from_be_slice(&ti[..32])
    } else {
        U256::ZERO
    };

    let mut data = json!({
        "epoch_id": epoch_id,
        "word_id": word_id,
        "tx_hash": tx_hash,
        "token_id_estimate": total.to_string(),
    });
    let mut message = format!("🎉 Inscribed Ardinal #{total} — tx={tx_hash}");
    if let Some((warn_payload, warn_msg)) =
        crate::cmd::gas::low_balance_warning(&agent_str)
    {
        data["balance_warning"] = warn_payload;
        message = format!("{message}\n\n{warn_msg}");
    }

    Output::success(
        message,
        data,
        Internal {
            next_action: "celebrate".into(),
            next_command: Some("ardi-agent commits".into()),
            ..Default::default()
        },
    )
    .print();
    Ok(())
}
