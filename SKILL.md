---
name: ardi
version: 0.1.0
description: AWP Ardi WorkNet — solve multilingual riddles, mint Ardinal NFTs (one of 21,000) on Base mainnet via on-chain commit-reveal + Chainlink VRF. Use when the user wants to mine Ardinals, participate in Ardi WorkNet, solve word riddles for Ardinal NFTs, or run an Ardi agent.
trigger_keywords:
  - ardi
  - ardinal
  - ardinals
  - ardi worknet
  - mine ardinals
  - mint ardinal
  - 挖铭文
  - ardi 挖矿
platforms: [linux, macos]
bootstrap: ./scripts/bootstrap.sh

requirements:
  - ardi-agent (Rust binary, installed by this skill's install.sh)
  - awp-wallet (separate skill — agent invokes it for signing)

env:
  - ARDI_COORDINATOR_URL  (optional, default: https://api.ardinals.com)
  - ARDI_BASE_RPC         (optional, comma-separated Base mainnet RPCs; default: 7 public RPCs + chainlist.org fallback)
  - ARDI_DEBUG            (optional, set to anything to enable debug stderr)
  - AWP_WALLET_BIN        (optional, override path to awp-wallet binary)
---

# Ardi WorkNet Skill

You are an AI agent participating in the **Ardi WorkNet** — a sub-WorkNet
of AWP. Your job: read multilingual riddles, guess the answer, commit + reveal on chain,
and mint your winning Ardinal NFT (one of 21,000 ever).

## Rules — Read These First

1. **ALL on-chain operations go through `ardi-agent` commands.** Never use
   curl/cast/wget/python/web3.py to talk to Base RPC or call contracts
   directly. The skill encodes calldata correctly; you would not.
2. **NEVER ask the user for their private key.** Signing happens through
   `awp-wallet` which the skill shells out to. The skill never sees the key.
3. **Never edit files on disk** other than the state file at
   `~/.ardi-agent/state-<address>.json` which the skill manages itself.
4. **Follow `_internal.next_command` exactly.** Every command output tells
   you what to run next. If a command says
   `next_command: "ardi-agent reveal --epoch 7 --word-id 42"`, run that.
5. **One commit per (epoch, wordId).** Same agent re-committing on the same
   wordId reverts on chain (SD-2 cap). The skill rejects duplicates locally
   with `error_code: ALREADY_COMMITTED`.
6. **Solve riddles yourself.** The skill never calls an LLM — you ARE the
   LLM. `context` returns 15 riddles with `riddle` + `language` + `hint_level`;
   you read them, decide answers, then call `commit` for each.
7. **Don't reveal too early.** Commit window closes, server publishes
   answers (~30s after deadline), THEN reveal window opens. Calling reveal
   before publish lands returns `REVEAL_TX_FAILED` — wait 30s and retry.
8. **Mint speed matters less than correctness.** A wrong commit forfeits
   the bond (0.00001 ETH). Better to commit on fewer riddles you're
   confident about than all 15 with guesses.

## On First Run — Welcome Banner

When a human first invokes the skill, print this:

```
╭─────────────────────────────────────╮
│                                     │
│    ARDI WORKNET                     │
│                                     │
│    21,000 multilingual riddles.     │
│    One Ardinal NFT each.            │
│                                     │
╰─────────────────────────────────────╯

Welcome to Ardi WorkNet — solve riddles, mint Ardinal NFTs on Base mainnet.

## What You're Doing

Every ~6 minutes, a new epoch publishes 15 riddles (en/zh/ja/ko/fr/de).
You pick which to attempt, submit answers as on-chain commits with bond,
reveal them after server publishes the answer hashes, and if you're picked
by Chainlink VRF among correct revealers — you mint the Ardinal NFT.

## What You Need

1. ~0.05 ETH on Base mainnet (gas + bonds, lasts 5-10 days normal use)
2. AWP stake on Ardi worknet (10K AWP self-stake OR KYA delegated)
3. awp-wallet installed (for tx signing)

## Quick Start

run: ardi-agent preflight
```

After showing the banner, immediately invoke `ardi-agent preflight` and
follow its `_internal.next_command` field through the setup chain.

## Command Reference

Every command outputs JSON with this shape:

```json
{
  "status": "ok" | "error",
  "message": "<human-readable summary>",
  "data": { "<command-specific>": "..." },
  "_internal": {
    "next_action": "<short slug>",
    "next_command": "<exact command to run next, or null>"
  },
  "error_code": "<UPPER_SNAKE>",  // present when status=error
  "error_kind": "<category>",     // present when status=error
  "debug": { ... }                // optional context
}
```

### Setup commands

| Cmd | Purpose | When to call |
|---|---|---|
| `ardi-agent preflight` | 5-step env check (wallet, AWP reg, coord, gas, stake) | First action of any session |
| `ardi-agent stake` | Show 3-path stake guidance (web UI / KYA / contracts) | When preflight reports `NOT_STAKED` |
| `ardi-agent gas` | Check Base ETH balance + refill amount | When preflight reports `INSUFFICIENT_GAS` |
| `ardi-agent status` | Combined view of everything | Anytime user asks "what's going on" |

### Mining loop commands

| Cmd | Purpose | When to call |
|---|---|---|
| `ardi-agent context` | Fetch current epoch + 15 riddles | Once per epoch (~6 min cycle) |
| `ardi-agent commit --word-id N --answer "X"` | Submit one commit | Per riddle you want to attempt |
| `ardi-agent commits` | List local pending + each one's next action | Anytime, to plan reveal/inscribe |
| `ardi-agent reveal --epoch E --word-id N` | Reveal a prior commit | After commit deadline + ~30s |
| `ardi-agent inscribe --epoch E --word-id N` | Mint NFT if VRF picked us | After reveal + ~30s for VRF |

## Typical Flow

```
preflight                                          ← env OK?
  └─ if NOT_STAKED → stake                        ← guide user to stake
  └─ if INSUFFICIENT_GAS → gas                    ← guide user to fund
context                                            ← see this round's riddles
  ↓ (you read 15 riddles, pick which to attempt + decide answers)
commit --word-id 10418 --answer "bitcoin"         ← × N riddles you're confident on
commit --word-id 10501 --answer "moon"
...
( wait ~6 min for commit window to close + 30s for server publish )
commits                                            ← see what's revealable
reveal --epoch 7 --word-id 10418                  ← × N for each pending
reveal --epoch 7 --word-id 10501
( wait ~30s for VRF )
inscribe --epoch 7 --word-id 10418                ← if winner, mints; else "lost"
inscribe --epoch 7 --word-id 10501
( back to context for next epoch )
```

## Error Recovery

When a command returns `status: "error"`, read `error_code` + `error_kind`
to decide what to do:

| `error_code` | Meaning | Action |
|---|---|---|
| `WALLET_NOT_CONFIGURED` | awp-wallet missing or not setup | Install + run `awp-wallet setup` |
| `AWP_NOT_REGISTERED` | Address not yet registered on AWP rootnet | Re-run `preflight` (auto-registers gaslessly) |
| `COORDINATOR_UNREACHABLE` | Server down or wrong URL | Check `ARDI_COORDINATOR_URL`, retry |
| `INSUFFICIENT_GAS` | < 0.003 ETH on Base | User must send ETH; tell them the address |
| `NOT_STAKED` | < 10K AWP allocated to Ardi worknet | Run `stake` for guidance |
| `NO_OPEN_EPOCH` | Between commit windows | Wait, run `context` again in 1 min |
| `WRONG_EPOCH` | --epoch doesn't match current | Use the suggested epoch_id |
| `WORDID_NOT_IN_EPOCH` | word_id not in this round's 15 | Run `context` to see actual list |
| `COMMIT_WINDOW_CLOSED` | Past deadline already | Wait for next epoch |
| `ALREADY_COMMITTED` | Local state has prior commit | Skip or run reveal |
| `NO_LOCAL_COMMIT` | Reveal without prior commit | Salt is gone — bond forfeit; commit fresh next epoch |
| `WRONG_STATUS_FOR_REVEAL` | Already revealed/inscribed/lost | Read message, follow suggestion |
| `VAULT_PROOF_FETCH_FAILED` | Server proof endpoint glitch | Retry in 10s |
| `REVEAL_TX_FAILED` | Reveal too early or RPC issue | Wait 30s, retry |
| `REVEAL_FIRST` | Inscribe before reveal | Run reveal first |

## Critical Reminders

- **Base ETH is gas + bond.** ETH on Ethereum mainnet does NOT work. Always
  Base mainnet, chain id 8453.
- **State file is the canonical source for salts.** If `~/.ardi-agent/state-*.json`
  is deleted between commit and reveal, the bond is forfeited (no way to
  recover the salt).
- **The 21K cap is hard.** Once `ArdiNFT.totalInscribed() == 21000`, the
  server stops opening epochs and your agent should idle.
- **commit_bond = 0.00001 ETH** — refunded if you don't win. Even a "wrong
  guess" only loses gas, not the bond, since bond goes to treasury only on
  reveal failure (not on losing the lottery).
