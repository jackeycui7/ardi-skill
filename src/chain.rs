// ============================================================================
// COPIED FROM coord-rs/crates/ardi-chain/src/abi.rs — keep in sync MANUALLY.
//
// The server is closed-source so we cannot pull this via crate or submodule.
// Whenever the server's abi.rs changes (struct fields, function selectors,
// event signatures), MIRROR THE CHANGES HERE and re-run the cross-check
// fixture in tests/abi_sync.rs.
//
// LAST SYNCED: 2026-04-30 (Phase 1 mining-only contracts)
// SOURCE     : /root/awp_code/ardi/coord-rs/crates/ardi-chain/src/abi.rs
// ============================================================================

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::sol;

sol! {
    #[allow(missing_docs)]
    contract ArdiEpochDraw {
        struct AnswerData {
            uint256 wordId;
            bytes32 wordHash;
            uint16 power;
            uint8 languageId;
            bytes32[] vaultProof;
        }

        function epochs(uint256 epochId) external view returns (
            uint64 startTs,
            uint64 commitDeadline,
            uint64 revealDeadline,
            bool exists
        );
        function getAnswer(uint256 epochId, uint256 wordId) external view returns (
            bytes32 wordHash,
            uint16 power,
            uint8 languageId,
            bool published
        );
        function commit(uint256 epochId, uint256 wordId, bytes32 hash) external payable;
        function reveal(
            uint256 epochId,
            uint256 wordId,
            string answer,
            bytes32 salt,
            bytes32[] vaultProof
        ) external;
        function winners(uint256 epochId, uint256 wordId) external view returns (address);
        function agentWinCount(address agent) external view returns (uint8);
    }
}

sol! {
    #[allow(missing_docs)]
    contract ArdiNFT {
        function ownerOf(uint256 tokenId) external view returns (address);
        function isSealed() external view returns (bool);
        function totalInscribed() external view returns (uint256);
        function inscribe(
            uint256 epochId,
            uint256 wordId,
            string word,
            bytes32 salt,
            uint16 power,
            uint8 languageId
        ) external returns (uint256);
    }
}

sol! {
    #[allow(missing_docs)]
    contract AWPRegistry {
        function getAgentInfo(address agent, uint256 worknetId) external view returns (
            bytes32 root,
            bool isValid,
            uint256 stake,
            address rewardRecipient
        );
    }
}

// ============================================================================
// Hash helpers — these MUST match the server side byte-for-byte. See
// tests/abi_sync.rs for fixtures comparing this to coord-rs output.
// ============================================================================

use sha3::{Digest, Keccak256};

/// commit hash = keccak256(abi.encodePacked(answer, salt, agent))
///   - answer: string (raw UTF-8 bytes, NO length prefix because encodePacked)
///   - salt:   bytes32 (32 raw bytes)
///   - agent:  address (20 raw bytes)
///
/// MUST match ArdiEpochDraw.commit's expected hash exactly.
pub fn commit_hash(answer: &str, salt: &B256, agent: &Address) -> B256 {
    let mut h = Keccak256::new();
    h.update(answer.as_bytes());
    h.update(salt.as_slice());
    h.update(agent.as_slice());
    B256::from_slice(&h.finalize())
}

/// Vault Merkle leaf = keccak256(abi.encodePacked(wordId, keccak256(word), power, languageId))
///   - wordId:     uint256 (32 bytes big-endian)
///   - keccak(word): bytes32 (raw)
///   - power:      uint16 (2 bytes BE)
///   - languageId: uint8 (1 byte)
pub fn vault_leaf(word_id: U256, word: &str, power: u16, language_id: u8) -> B256 {
    let word_hash = Keccak256::digest(word.as_bytes());
    let mut h = Keccak256::new();
    let word_id_be: [u8; 32] = word_id.to_be_bytes::<32>();
    h.update(word_id_be);
    h.update(word_hash);
    h.update(power.to_be_bytes());
    h.update([language_id]);
    B256::from_slice(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn commit_hash_fixture() {
        // Fixture vector — paired test on the server side must produce same.
        let answer = "bitcoin";
        let salt = B256::from([0x11u8; 32]);
        let agent = address!("46a1eee3d800799726faaf18f28360eb2e97ad63");
        let h = commit_hash(answer, &salt, &agent);
        // Compute expected: keccak256("bitcoin" || 0x11..11 || 0x46a1...)
        let mut k = Keccak256::new();
        k.update(b"bitcoin");
        k.update([0x11u8; 32]);
        k.update(agent.as_slice());
        let expected = B256::from_slice(&k.finalize());
        assert_eq!(h, expected);
    }
}
