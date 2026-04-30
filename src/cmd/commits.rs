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
    let st = State::load()?;
    let now = chrono::Utc::now().timestamp();

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

    Output::success(
        format!("{} pending commits ({})", rows.len(), next_action),
        json!({ "pending": rows }),
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
