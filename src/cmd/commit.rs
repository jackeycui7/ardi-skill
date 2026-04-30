// commit — sign + broadcast a commit tx for one (epoch, wordId, answer).
//
// The agent's LLM provides --word-id and --answer. We:
//   1. Generate random 32-byte salt
//   2. Compute commit_hash = keccak(answer || salt || agent)
//   3. Build commit calldata via ArdiEpochDraw::commitCall
//   4. send_tx via awp-wallet bridge with COMMIT_BOND value
//   5. Wait receipt
//   6. Persist (salt, answer) to ~/.ardi-agent/state.json so reveal can recover

use anyhow::{anyhow, Context, Result};
use alloy_primitives::{Address, B256};
use rand::RngCore;
use serde_json::json;
use std::str::FromStr;

use crate::auth::get_address;
use crate::chain;
use crate::client::ApiClient;
use crate::output::{Internal, Output};
use crate::state::{CommitStatus, PendingCommit, State};
use crate::tx;
use crate::log_info;

pub struct CommitArgs {
    pub epoch_id: Option<u64>, // if None, use current
    pub word_id: u64,
    pub answer: String,
}

pub fn run(server_url: &str, args: CommitArgs) -> Result<()> {
    let agent_str = match get_address() {
        Ok(a) => a,
        Err(e) => {
            Output::error(
                format!("Cannot resolve agent address: {e}"),
                "WALLET_NOT_CONFIGURED",
                "dependency",
                false,
                "Run `ardi-agent preflight` to set up your wallet.",
                Internal {
                    next_action: "configure_wallet".into(),
                    next_command: Some("ardi-agent preflight".into()),
                    ..Default::default()
                },
            )
            .print();
            return Ok(());
        }
    };
    let agent = Address::from_str(&agent_str)?;

    // Resolve epoch_id from current if not given.
    let api = ApiClient::new(server_url)?;
    let cur: Option<serde_json::Value> = api.try_get_json("/v1/epoch/current")?;
    let Some(cur) = cur else {
        Output::error(
            "No commit-able epoch right now (commit window for this cycle has closed; new epoch opens within ~6 min).",
            "NO_OPEN_EPOCH",
            "timing",
            true,
            "Wait for the next openEpoch and re-fetch with `ardi-agent context`.",
            Internal {
                next_action: "wait".into(),
                next_command: Some("ardi-agent context".into()),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    };
    let cur_epoch_id = cur.get("epoch_id").and_then(|v| v.as_u64()).unwrap_or(0);
    let epoch_id = args.epoch_id.unwrap_or(cur_epoch_id);
    if epoch_id != cur_epoch_id {
        Output::error(
            format!(
                "Epoch {epoch_id} is not the current commit-able epoch (current={cur_epoch_id})."
            ),
            "WRONG_EPOCH",
            "validation",
            false,
            format!("Use --epoch {cur_epoch_id} or omit --epoch to use current."),
            Internal {
                next_action: "fix_args".into(),
                next_command: Some(format!(
                    "ardi-agent commit --epoch {cur_epoch_id} --word-id {} --answer {}",
                    args.word_id, args.answer
                )),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    // Validate the riddle exists in the published list.
    let riddles = cur
        .get("riddles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let Some(riddle) = riddles
        .iter()
        .find(|r| r.get("word_id").and_then(|v| v.as_u64()) == Some(args.word_id))
    else {
        let available: Vec<u64> = riddles
            .iter()
            .filter_map(|r| r.get("word_id").and_then(|v| v.as_u64()))
            .collect();
        Output::error_with_debug(
            format!(
                "wordId {} is not in epoch {epoch_id}'s published riddles.",
                args.word_id
            ),
            "WORDID_NOT_IN_EPOCH",
            "validation",
            false,
            "Run `ardi-agent context` to see the actual wordIds for this epoch.",
            json!({ "epoch_id": epoch_id, "available_word_ids": available }),
            Internal {
                next_action: "rerun_context".into(),
                next_command: Some("ardi-agent context".into()),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    };

    let language = riddle
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("en")
        .to_string();
    let language_id = riddle
        .get("language_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;
    let power = riddle
        .get("power")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u16;

    // Commit window guard — reject early instead of paying gas for a tx
    // that will revert CommitWindowClosed.
    let commit_dl = cur.get("commit_deadline").and_then(|v| v.as_i64()).unwrap_or(0);
    let now = chrono::Utc::now().timestamp();
    if now >= commit_dl {
        Output::error(
            format!(
                "Commit window for epoch {epoch_id} closed {}s ago. Wait for the next epoch.",
                now - commit_dl
            ),
            "COMMIT_WINDOW_CLOSED",
            "timing",
            true,
            "Wait ~6min for next openEpoch then re-fetch with `ardi-agent context`.",
            Internal {
                next_action: "wait".into(),
                next_command: Some("ardi-agent context".into()),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    // Gas + bond pre-check.
    let gas = crate::cmd::gas::check_balance(&agent_str)?;
    let need_eth = (tx::COMMIT_BOND_WEI as f64) / 1e18 + 0.0005; // bond + ~0.5mETH gas headroom
    if gas.balance_eth < need_eth {
        Output::error(
            format!(
                "Wallet has {:.6} ETH; commit needs at least {:.6} ETH (bond {} wei + gas).",
                gas.balance_eth, need_eth, tx::COMMIT_BOND_WEI
            ),
            "INSUFFICIENT_GAS",
            "balance",
            true,
            format!("Send 0.05 ETH on Base mainnet to {agent_str}, then retry."),
            Internal {
                next_action: "fund_gas".into(),
                next_command: Some("ardi-agent gas".into()),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    // Idempotency: if local state already has a commit for this (epoch,
    // word_id), don't double-commit. Same wordId from same agent in same
    // epoch would also revert AlreadyCommitted on chain.
    let st_check = State::load()?;
    if let Some(existing) = st_check.get(epoch_id, args.word_id) {
        Output::error(
            format!(
                "Already committed on (epoch={epoch_id}, word_id={}) with answer \"{}\" at tx {}.",
                args.word_id, existing.answer, existing.commit_tx
            ),
            "ALREADY_COMMITTED",
            "state",
            false,
            format!(
                "If reveal window has opened, run `ardi-agent reveal --epoch {epoch_id} --word-id {}`. \
                 To re-try with a different answer is impossible — same agent can't re-commit on the same wordId per SD-2.",
                args.word_id
            ),
            Internal {
                next_action: "skip_or_reveal".into(),
                next_command: Some(format!(
                    "ardi-agent reveal --epoch {epoch_id} --word-id {}",
                    args.word_id
                )),
                ..Default::default()
            },
        )
        .print();
        return Ok(());
    }

    // Generate salt.
    let mut salt_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let salt = B256::from(salt_bytes);

    // Hash MUST match server validation byte-for-byte.
    let hash = chain::commit_hash(&args.answer, &salt, &agent);
    log_info!(
        "commit: epoch={epoch_id} word={} hash=0x{}",
        args.word_id,
        hex::encode(hash)
    );

    // Build calldata + tx.
    let to = Address::from_str(
        cur.get("epoch_draw_contract")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("server didn't return epoch_draw_contract"))?,
    )?;
    let data = tx::calldata_commit(epoch_id, args.word_id, hash);
    let tx_obj = tx::build_tx(&agent, &to, data, tx::COMMIT_BOND_WEI, 200_000)?;

    let tx_hash = tx::send_and_wait(&tx_obj).context("send commit tx")?;
    log_info!("commit: tx submitted {tx_hash}");

    // Persist BEFORE waiting receipt — losing this state means we forget the
    // salt and can never reveal, which would forfeit the bond. Better to
    // record optimistically; if tx reverts we can update status to Failed.
    let mut st = State::load()?;
    st.put(PendingCommit {
        epoch_id,
        word_id: args.word_id,
        answer: args.answer.clone(),
        salt_hex: format!("0x{}", hex::encode(salt)),
        agent: agent_str.clone(),
        commit_tx: tx_hash.clone(),
        commit_hash: format!("0x{}", hex::encode(hash)),
        committed_at: chrono::Utc::now().timestamp(),
        language,
        power,
        language_id,
        status: CommitStatus::Committed,
        reveal_tx: None,
        inscribe_tx: None,
        token_id: None,
    });
    st.save()?;

    Output::success(
        format!(
            "Commit submitted: epoch={epoch_id} word={} tx={tx_hash}. Wait until reveal window opens, then `ardi-agent reveal --epoch {epoch_id} --word-id {}`.",
            args.word_id, args.word_id
        ),
        json!({
            "epoch_id": epoch_id,
            "word_id": args.word_id,
            "tx_hash": tx_hash,
            "commit_hash": format!("0x{}", hex::encode(hash)),
            "bond_wei": tx::COMMIT_BOND_WEI.to_string(),
            "reveal_after": cur.get("commit_deadline"),
        }),
        Internal {
            next_action: "wait_reveal".into(),
            next_command: Some(format!(
                "ardi-agent reveal --epoch {epoch_id} --word-id {}",
                args.word_id
            )),
            ..Default::default()
        },
    )
    .print();
    Ok(())
}
