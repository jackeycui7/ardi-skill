// context — fetch the current commit-able epoch + riddles from coord-rs.

use anyhow::Result;
use serde_json::json;

use crate::client::ApiClient;
use crate::output::{Internal, Output};

pub fn run(server_url: &str) -> Result<()> {
    let api = ApiClient::new(server_url)?;
    let body: Option<serde_json::Value> = api.try_get_json("/v1/epoch/current")?;
    match body {
        Some(epoch) => {
            let id = epoch.get("epoch_id").and_then(|v| v.as_u64()).unwrap_or(0);
            let cd = epoch.get("commit_deadline").and_then(|v| v.as_i64()).unwrap_or(0);
            let now = chrono::Utc::now().timestamp();
            let secs_left = cd - now;
            Output::success(
                format!(
                    "Epoch {id}: commit window closes in {secs_left}s ({} riddles).",
                    epoch.get("riddles").and_then(|r| r.as_array()).map(|a| a.len()).unwrap_or(0)
                ),
                epoch,
                Internal {
                    next_action: "solve_then_commit".into(),
                    next_command: Some("ardi-agent mine".into()),
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
