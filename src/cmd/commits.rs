// commits — list local pending commits + per-row next_command.
//
// This is what an agent calls between rounds to figure out "what should I
// do next?" — it surfaces commits that need reveal (window open) or
// inscribe (winner picked).

use anyhow::Result;
use serde_json::json;

use crate::output::{Internal, Output};
use crate::state::{CommitStatus, State};

pub fn run(_server_url: &str) -> Result<()> {
    let mut st = State::load()?;
    let now = chrono::Utc::now().timestamp();

    // Backfill: any Inscribed/Won row that's missing token_id was written by
    // pre-v0.5.9 inscribe (which never recorded the tokenId). V3 derives
    // tokenId = wordId + 1 deterministically, so we can repair retroactively
    // without an RPC call. This eliminates the "ardi-agent commits says
    // token_id:null but ardi-agent status / chain says #1869 is mine" lie
    // that misled testers into thinking they hadn't won.
    let mut backfilled = 0;
    for c in st.pending.values_mut() {
        if matches!(c.status, CommitStatus::Inscribed | CommitStatus::Won) && c.token_id.is_none() {
            c.token_id = Some(c.word_id + 1);
            backfilled += 1;
        }
    }
    if backfilled > 0 {
        st.save()?;
    }

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for c in st.pending.values() {
        let suggested_next = match c.status {
            CommitStatus::Committed => Some(format!(
                "ardi-agent reveal --epoch {} --word-id {} (after commit_deadline + publish)",
                c.epoch_id, c.word_id
            )),
            CommitStatus::Revealed => Some(format!(
                "ardi-agent inscribe --epoch {} --word-id {}",
                c.epoch_id, c.word_id
            )),
            CommitStatus::Won => Some(format!(
                "ardi-agent inscribe --epoch {} --word-id {}",
                c.epoch_id, c.word_id
            )),
            _ => None,
        };
        rows.push(json!({
            "epoch_id": c.epoch_id,
            "word_id": c.word_id,
            "status": format!("{:?}", c.status).to_lowercase(),
            "answer": c.answer,
            "committed_at": c.committed_at,
            "age_seconds": now - c.committed_at,
            "commit_tx": c.commit_tx,
            "reveal_tx": c.reveal_tx,
            "inscribe_tx": c.inscribe_tx,
            "token_id": c.token_id,
            "next_command": suggested_next,
        }));
    }

    let next_action = if rows
        .iter()
        .any(|r| r.get("status").and_then(|v| v.as_str()) == Some("revealed"))
    {
        "inscribe_pending"
    } else if rows
        .iter()
        .any(|r| r.get("status").and_then(|v| v.as_str()) == Some("committed"))
    {
        "reveal_pending"
    } else {
        "idle"
    };

    let mut data = json!({ "pending": rows });
    let mut message = format!("{} pending commits ({})", rows.len(), next_action);

    // Wallet balance reminder — agents tend to forget topping up.
    if let Ok(addr) = crate::auth::get_address() {
        if let Some((warn_payload, warn_msg)) = crate::cmd::gas::low_balance_warning(&addr) {
            data["balance_warning"] = warn_payload;
            message = format!("{message}\n\n{warn_msg}");
        }
    }

    Output::success(
        message,
        data,
        Internal {
            next_action: next_action.into(),
            next_command: rows
                .iter()
                .find_map(|r| r.get("next_command").and_then(|v| v.as_str()))
                .map(String::from),
            ..Default::default()
        },
    )
    .print();
    Ok(())
}
