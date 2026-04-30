// context — fetch the current commit-able epoch + riddles from coord-rs.

use anyhow::Result;
use serde_json::json;

use crate::client::ApiClient;
use crate::output::{Internal, Output};

pub fn run(server_url: &str) -> Result<()> {
    let api = ApiClient::new(server_url)?;
    let body: Option<serde_json::Value> = api.try_get_json("/v1/epoch/current")?;
    match body {
        Some(mut epoch) => {
            let id = epoch.get("epoch_id").and_then(|v| v.as_u64()).unwrap_or(0);
            let cd = epoch.get("commit_deadline").and_then(|v| v.as_i64()).unwrap_or(0);
            let now = chrono::Utc::now().timestamp();
            let secs_left = cd - now;
            let mut message = format!(
                "Epoch {id}: commit window closes in {secs_left}s ({} riddles).",
                epoch.get("riddles").and_then(|r| r.as_array()).map(|a| a.len()).unwrap_or(0)
            );
            // Reminder — without enough gas the agent can't actually commit.
            if let Ok(addr) = crate::auth::get_address() {
                if let Some((warn_payload, warn_msg)) =
                    crate::cmd::gas::low_balance_warning(&addr)
                {
                    epoch["balance_warning"] = warn_payload;
                    message = format!("{message}\n\n{warn_msg}");
                }
            }
            Output::success(
                message,
                epoch,
                Internal {
                    next_action: "solve_then_commit".into(),
                    next_command: Some(
                        "ardi-agent commit --word-id <ID> --answer \"<your answer>\"".into(),
                    ),
                    ..Default::default()
                },
            )
            .print();
        }
        None => {
            Output::success(
                "No epoch is currently in commit window. Wait for the next openEpoch (~6 min cycle).",
                json!({ "current": null }),
                Internal {
                    next_action: "wait".into(),
                    next_command: Some("ardi-agent context".into()),
                    ..Default::default()
                },
            )
            .print();
        }
    }
    Ok(())
}
