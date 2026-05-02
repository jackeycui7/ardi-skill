// ============================================================================
// COPIED FROM coord-rs/crates/ardi-chain/src/abi.rs — keep in sync MANUALLY.
//
// The server is closed-source so we cannot pull this via crate or submodule.
// Whenever the server's abi.rs changes (struct fields, function selectors,
// event signatures), MIRROR THE CHANGES HERE.
//
// LAST SYNCED: 2026-05-02 (v3.1 contracts: leaf encoding switched to abi.encode,
//              AnswerData gained themeHash + elementHash, element max raised
//              5→6 for god-tier, ArdiNFTv3 ELEMENT_MAX bumped to 6).
// SOURCE     : /root/awp_code/ardi/coord-rs/crates/ardi-chain/src/abi.rs
//              + /root/awp_code/ardi/contracts-v2/src/v3/*.sol
//
// Skill never calls publishAnswer (only the coordinator does), so AnswerData
// + vault_leaf are intentionally NOT mirrored here. Skill's hot path is:
//   commit() → reveal() → inscribe() — the structs and leaf encoding for
// vault Merkle proof live entirely server-side / on-chain.
// ============================================================================

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::sol;

sol! {
    #[allow(missing_docs)]
    contract ArdiEpochDraw {

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
        // v3.1: commit takes a staker LIST (max 8, strict ascending = dedup).
        // Empty array → self-stake fallback (msg.sender as the only staker).
        function commit(
            uint256 epochId,
            uint256 wordId,
            bytes32 hash,
            address[] stakers
        ) external payable;
        function getCommitStakers(uint256 epochId, uint256 wordId, address agent)
            external view returns (address[] memory);
        function liveStakeForCommit(uint256 epochId, uint256 wordId, address agent)
            external view returns (uint256);
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
    contract ArdiOTC {
        struct Listing {
            address seller;
            uint256 priceWei;
            uint64 listedAt;
        }
        function list(uint256 tokenId, uint256 priceWei) external;
        function unlist(uint256 tokenId) external;
        function buy(uint256 tokenId) external payable;
        function getListing(uint256 tokenId) external view returns (Listing memory);
        function isListed(uint256 tokenId) external view returns (bool);
    }
}

sol! {
    #[allow(missing_docs)]
    contract IERC721 {
        function setApprovalForAll(address operator, bool approved) external;
        function isApprovedForAll(address owner, address operator) external view returns (bool);
        function getApproved(uint256 tokenId) external view returns (address);
        function approve(address to, uint256 tokenId) external;
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

// vault_leaf removed in v3.1: skill never builds vault Merkle proofs. The
// coordinator publishes answers; agents only commit/reveal/inscribe. The
// canonical leaf lives in coord-rs/ardi-core/src/vault.rs::vault_leaf.

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
