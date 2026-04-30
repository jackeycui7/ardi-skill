// Local state — tracks pending commits per agent.
//
// A commit on chain only carries hash(answer, salt, agent). To reveal we
// need to remember the plaintext (answer, salt) we committed with. Stored
// at ~/.ardi-agent/state.json, owned by the user (no daemon, no rotation).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PendingCommit {
    pub epoch_id: u64,
    pub word_id: u64,
    pub answer: String,
    pub salt_hex: String,        // 0x-prefixed 32-byte hex
    pub agent: String,           // 0x-lower
    pub commit_tx: String,       // 0x hash
    pub commit_hash: String,     // 0x bytes32 we computed
    pub committed_at: i64,       // unix seconds
    pub language: String,
    pub power: u16,
    pub language_id: u8,
    pub status: CommitStatus,
    pub reveal_tx: Option<String>,
    pub inscribe_tx: Option<String>,
    pub token_id: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitStatus {
    Committed,
    Revealed,
    /// VRF picked us as winner; ready to inscribe.
    Won,
    /// VRF picked someone else; nothing more to do.
    Lost,
    /// Already minted into ArdiNFT.
    Inscribed,
    /// Anything that went wrong — kept for debugging.
    Failed,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct State {
    /// Keyed "{epoch_id}:{word_id}" so it round-trips JSON cleanly.
    #[serde(default)]
    pub pending: BTreeMap<String, PendingCommit>,
}

impl State {
    pub fn load() -> Result<Self> {
        let p = path();
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&p)
            .with_context(|| format!("read {}", p.display()))?;
        let s = serde_json::from_str(&raw).unwrap_or_default();
        Ok(s)
    }

    pub fn save(&self) -> Result<()> {
        let p = path();
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).ok();
        }
        let tmp = p.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(tmp, &p)?;
        Ok(())
    }

    pub fn key(epoch_id: u64, word_id: u64) -> String {
        format!("{epoch_id}:{word_id}")
    }

    pub fn put(&mut self, c: PendingCommit) {
        self.pending.insert(Self::key(c.epoch_id, c.word_id), c);
    }

    pub fn get(&self, epoch_id: u64, word_id: u64) -> Option<&PendingCommit> {
        self.pending.get(&Self::key(epoch_id, word_id))
    }

    pub fn get_mut(&mut self, epoch_id: u64, word_id: u64) -> Option<&mut PendingCommit> {
        self.pending.get_mut(&Self::key(epoch_id, word_id))
    }
}

/// Per-agent state file path. Each agent address gets its own file so two
/// wallets on the same machine never clash. Falls back to `state.json` when
/// no address resolved yet (preflight not yet run).
fn path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let dir = PathBuf::from(home).join(".ardi-agent");
    let addr = crate::auth::get_address().ok();
    let fname = match addr {
        Some(a) => format!("state-{}.json", a.to_lowercase()),
        None => "state.json".to_string(),
    };
    dir.join(fname)
}
