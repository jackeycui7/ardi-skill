// ============================================================================
// COPIED FROM coord-rs/crates/ardi-chain/src/abi.rs — keep in sync MANUALLY.
//
// The server is closed-source so we cannot pull this via crate or submodule.
// Whenever the server's abi.rs changes (struct fields, function selectors,
// event signatures), MIRROR THE CHANGES HERE.
//
// LAST SYNCED: 2026-05-01 (v3 contracts: NFT v3 + EmissionDistributor + EpochDraw v3)
// SOURCE     : /root/awp_code/ardi/coord-rs/crates/ardi-chain/src/abi.rs
//              + /root/awp_code/ardi/contracts-v2/src/v3/*.sol
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
            uint8 maxDurability;
            uint8 element;
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
            uint8 maxDurability,
            uint8 element,
            bool published
        );
        // v3: commit takes staker. address(0) → msg.sender (self-stake).
        function commit(
            uint256 epochId,
            uint256 wordId,
            bytes32 hash,
            address staker
        ) external payable;
        // v3 reveal — only (guess, nonce). vaultProof goes to publishAnswers, not reveal.
        function reveal(
            uint256 epochId,
            uint256 wordId,
            string guess,
            bytes32 nonce
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
        // v3 inscribe — only (epoch, wordId, word). Power, lang, durability,
        // element are sourced from EpochDraw.getAnswer on chain.
        function inscribe(uint64 epochId, uint256 wordId, string word) external;
        // ERC721 transfer — used by `ardi-agent transfer` to move NFTs from
        // the agent's wallet to the user's main wallet (e.g. MetaMask) so
        // they can repair / claim from the browser instead of CLI. Reverts
        // with `TokenLocked` if the NFT has a pending repair or fuse VRF.
        function transferFrom(address from, address to, uint256 tokenId) external;
        function pendingRepairOf(uint256 tokenId) external view returns (uint256);
        function pendingFuseOf(uint256 tokenId) external view returns (uint256);
        // v3 repair — pay fee + request VRF. Returns the requestId.
        function repair(uint256 tokenId) external returns (uint256);
        function repairFee(uint256 tokenId) external view returns (uint256);
        function effectiveDurability(uint256 tokenId) external view returns (uint8);
    }
}

sol! {
    #[allow(missing_docs)]
    contract EmissionDistributor {
        function pendingFor(address holder, uint256[] tokenIds)
            external view returns (uint256);
        function claim(uint256[] tokenIds) external;
    }
}

sol! {
    #[allow(missing_docs)]
    // v3: replaces AWPRegistry. Stake check goes through AWPAllocator since
    // the registry's getAgentInfo only sees stake when the agent has called
    // bind(staker), which KYA-delegated agents never do. The allocator query
    // takes the staker explicitly.
    contract AWPAllocator {
        function getAgentStake(
            address staker,
            address agent,
            uint256 worknetId
        ) external view returns (uint256);
    }
}

sol! {
    #[allow(missing_docs)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
    }
}

// ============================================================================
// Hash helpers — these MUST match the server side byte-for-byte.
// ============================================================================

use sha3::{Digest, Keccak256};

/// commit hash = keccak256(abi.encodePacked(answer, agent, nonce))
///
/// MUST match ArdiEpochDraw.reveal's expected hash exactly:
///   bytes32 expected = keccak256(abi.encodePacked(guess, msg.sender, nonce));
///
/// NOTE: contract field order is (guess, msg.sender, nonce). Matching that
/// order here.
pub fn commit_hash(answer: &str, agent: &Address, nonce: &B256) -> B256 {
    let mut h = Keccak256::new();
    h.update(answer.as_bytes());
    h.update(agent.as_slice());
    h.update(nonce.as_slice());
    B256::from_slice(&h.finalize())
}

/// v3 vault Merkle leaf =
///   keccak256(abi.encodePacked(
///     uint256 wordId, bytes32 keccak(word),
///     uint16 power, uint8 languageId,
///     uint8 maxDurability, uint8 element
///   ))
pub fn vault_leaf(
    word_id: U256,
    word: &str,
    power: u16,
    language_id: u8,
    max_durability: u8,
    element: u8,
) -> B256 {
    let word_hash = Keccak256::digest(word.as_bytes());
    let mut h = Keccak256::new();
    let word_id_be: [u8; 32] = word_id.to_be_bytes::<32>();
    h.update(word_id_be);
    h.update(word_hash);
    h.update(power.to_be_bytes());
    h.update([language_id]);
    h.update([max_durability]);
    h.update([element]);
    B256::from_slice(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn commit_hash_matches_v3_layout() {
        let answer = "bitcoin";
        let agent = address!("46a1eee3d800799726faaf18f28360eb2e97ad63");
        let nonce = B256::from([0x11u8; 32]);
        let h = commit_hash(answer, &agent, &nonce);
        let mut k = Keccak256::new();
        k.update(b"bitcoin");
        k.update(agent.as_slice());
        k.update([0x11u8; 32]);
        let expected = B256::from_slice(&k.finalize());
        assert_eq!(h, expected);
    }
}
