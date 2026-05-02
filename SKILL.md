---
name: ardi
version: 0.3.0
description: AWP Ardi WorkNet — solve multilingual riddles, mint Ardinal NFTs (one of 21,000) on Base mainnet via on-chain commit-reveal + Chainlink VRF. Use when the user wants to mine Ardinals, participate in Ardi WorkNet, solve word riddles for Ardinal NFTs, or run an Ardi agent.
license: MIT
homepage: https://github.com/jackeycui7/ardi-skill
platforms: [linux, macos]
tags: [web3, base, nft, riddle, awp, ardinal, mining]
category: web3

# Top-level shared trigger hints (Claude Skills / agentskills.io read these).
trigger_keywords:
  - ardi
  - ardinal
  - ardinals
  - ardi worknet
  - mine ardinals
  - mint ardinal

# Bootstrap/smoke entrypoints — every major runtime looks for these
# at these top-level keys.
bootstrap: ./scripts/bootstrap.sh
smoke_test: ./scripts/smoke_test.sh

# ── Hermes (nousresearch.com) — metadata.hermes ────────────────────
metadata:
  hermes:
    tags: [web3, base, nft, riddle, awp, ardinal]
    category: web3
    requires_toolsets: [terminal]
    required_environment_variables:
      - name: ARDI_COORDINATOR_URL
        prompt: Ardi coordinator API base URL
        help: Defaults to https://api.ardinals.com. Override only if you run a private coord.
        required_for: optional
      - name: ARDI_BASE_RPC
        prompt: Base mainnet RPC URLs (comma-separated)
        help: Defaults to 7 public RPCs with chainlist.org fallback. Override to use a private RPC.
        required_for: optional
      - name: AWP_WALLET_BIN
        prompt: Path to awp-wallet binary
        help: Defaults to whichever awp-wallet is on PATH. Override only if you have multiple installs.
        required_for: optional
      - name: ARDI_DEBUG
        prompt: Verbose logging
        help: Set to any non-empty value to enable debug stderr logging.
        required_for: optional
    config:
      - key: ardi.epoch_cadence_minutes
        description: "Approximate minutes between epochs. Currently 6 — informational only, the server controls actual cadence."
        default: "6"
        prompt: "Epoch cadence (informational, server-driven)"

  # ── OpenClaw — metadata.openclaw ─────────────────────────────────
  openclaw:
    bootstrap: ./scripts/bootstrap.sh
    smoke_test: ./scripts/smoke_test.sh
    requires:
      bins:
        - ardi-agent       # installed by bootstrap.sh / install.sh if missing
      anyBins:
        - awp-wallet       # installed by awp-wallet skill if missing
      skills:
        - https://github.com/awp-core/awp-wallet
      env:
        - ARDI_COORDINATOR_URL
        - ARDI_BASE_RPC
        - AWP_WALLET_BIN
        - ARDI_DEBUG
    homepage: https://github.com/jackeycui7/ardi-skill
    emoji: "🪨"
    install:
      - kind: script
        run: ./scripts/bootstrap.sh
    security:
      wallet_bridge:
        no_direct_key_access: false  # Skill calls awp-wallet export-private-key, holds key in process memory just long enough to sign one tx
        contract_allowlist: false    # Skill talks ONLY to ArdiNFT + ArdiEpochDraw on Base mainnet 8453; addresses are compiled into the binary
        session_token_only: false    # awp-wallet is unlocked-by-default; no session token model
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

When a human first invokes the skill, print this welcome message.
**Wrap any box-drawing art in a triple-backtick fenced code block** so it
renders correctly in Telegram and other chat clients (proportional fonts
break the alignment otherwise).

Recommended chat-friendly version (no box art):

```
**ARDI WORKNET** — 21,000 multilingual riddles, one Ardinal NFT each.

Every ~6 min a new epoch publishes 15 riddles (en/zh/ja/ko/fr/de). You
read them, commit your answers on chain with a small bond, reveal after
the server publishes the answer hashes, and if Chainlink VRF picks you
among correct revealers — you mint the Ardinal NFT.

What you need:
- ~0.05 ETH on Base mainnet (gas + bonds, lasts 5-10 days)
- 10,000 AWP staked on Ardi worknet (or KYA delegated path — no AWP needed)
- awp-wallet installed for tx signing

Run: `ardi-agent preflight`
```

After showing the welcome, immediately invoke `ardi-agent preflight` and
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
| `ardi-agent stake` | Show 3-path stake guidance (KYA / buy-and-stake / manual) | When preflight reports `NOT_STAKED` |
| `ardi-agent buy-and-stake --quote` | **CALL THIS FIRST.** Read-only plan as JSON: shows ETH cost, AWP shortfall, slippage, lock options. NO on-chain action. Relay the plan to the user, ask "OK to spend X ETH? Lock for how many days (default 3)?". | When user has ETH but no AWP — STEP 1 of two-step UX |
| `ardi-agent buy-and-stake --yes --lock-days N` | After user confirms the quote, this actually executes (swap + deposit + allocate). `-y` skips the on-stdin prompt because LLM agents have no interactive stdin; `--lock-days` is required (no default at execute time, you must pass the user's chosen number). Optional `--slippage BPS` if user wants something other than 3%. | STEP 2 — only after user explicitly confirms the quote from STEP 1 |
| `ardi-agent gas` | Check Base ETH balance + refill amount | When preflight reports `INSUFFICIENT_GAS` |
| `ardi-agent status` | Combined view of everything | Anytime user asks "what's going on" |

### **Critical: commits MUST be serial, never parallel**

`ardi-agent commit` is a one-shot process that fetches its tx nonce
fresh from the chain. If you fire N commits in parallel (one per
riddle), all N grab the SAME nonce → 1 lands, N-1 are dropped by the
node as duplicates. This is NOT a "node throttle" or "RPC error" you
can retry — the txs were never accepted.

Correct pattern:

```bash
# WRONG — parallel, will lose ~14 of 15
ardi-agent commit --word-id A --answer X &
ardi-agent commit --word-id B --answer Y &
wait

# RIGHT — serial, all 15 land
ardi-agent commit --word-id A --answer X
ardi-agent commit --word-id B --answer Y
ardi-agent commit --word-id C --answer Z
...
```

If you have many commits to send, **await each** before launching the
next. A typical commit takes ~3-5s including receipt wait, so 5
commits ≈ 15-25s — well within the 180s commit window.

For unattended bulk mining, do NOT loop manually — install the
`tools/auto-mine/` systemd timer (see "Autonomous mining mode" below).
That tool already handles serial nonce management and retry-on-revert.

### Mining loop commands

| Cmd | Purpose | When to call |
|---|---|---|
| `ardi-agent context` | Fetch current epoch + 15 riddles | Once per epoch (~6 min cycle) |
| `ardi-agent commit --word-id N --answer "X"` | Submit one commit | Per riddle you want to attempt |
| `ardi-agent commits` | List local pending + each one's next action | Anytime, to plan reveal/inscribe |
| `ardi-agent reveal --epoch E --word-id N` | Reveal a prior commit | After commit deadline + ~30s |
| `ardi-agent inscribe --epoch E --word-id N` | Mint NFT if VRF picked us | After reveal + ~30s for VRF |

## Autonomous mining mode

**A built-in tool exists. NEVER write your own loop / cron / watcher
script for Ardi mining — use the built-in `tools/auto-mine/` instead.
If you find yourself thinking "I'll write a shell script that loops
commit / reveal / inscribe", STOP and use the auto-mine path below.**

Trigger this whenever the user expresses ANY of these intents (English
OR another language with the same meaning — translate the intent, not
the literal phrase):

- Direct: "mine continuously" / "auto mine" / "I want to be afk"
- Implied: "keep mining for me" / "run forever" / "just go" / "you mine
  for me" / "leave it running" / "watch the epochs" / "schedule it"
- Question form: "can you mine automatically?" / "is there a way to
  loop this?" / "do you have a daemon mode?"
- Workflow setup: "set up the cron" / "start the daemon" / "set up the
  pipeline" / "make it self-running"

Even when the user only asks about ONE cycle, if their intent reads
like "I want to do this repeatedly without manual prompts," install
auto-mine PROACTIVELY and tell them you did.

Install — **do not have the user type any shell commands** — call this
script directly:

```bash
~/.local/share/ardi-auto-mine/install.sh
# OR if not yet installed (first time):
git clone https://github.com/jackeycui7/ardi-skill ~/ardi-skill 2>/dev/null || \
  (cd ~/ardi-skill && git pull)
~/ardi-skill/tools/auto-mine/install.sh
```

`install.sh` is fully non-interactive:
- Auto-detects which runtime CLI is installed (claude / hermes / openclaw)
- Writes a minimal env file with sensible defaults (no API key required —
  the runtime CLI uses its own credentials)
- Installs systemd user units
- Auto-starts the timer

After install, periodically check status when the user asks ("how's it
going?", "any wins?") OR proactively every few hours:

```bash
~/.local/share/ardi-auto-mine/status.sh   # → JSON snapshot
```

Narrate the result naturally — don't paste raw JSON.

To pause / resume / uninstall:

```bash
~/.local/share/ardi-auto-mine/stop.sh         # pause
systemctl --user start ardi-mine.timer        # resume
~/.local/share/ardi-auto-mine/uninstall.sh    # full uninstall
```

## Typical Flow

```
preflight                                          ← env OK?
  └─ if NOT_STAKED + has ETH  → buy-and-stake    ← one-command auto-onboard
  └─ if NOT_STAKED + no ETH   → stake            ← show 3 paths (KYA recommended)
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
| `NOT_STAKED` | < 10K AWP allocated to Ardi worknet | If user has ETH: `buy-and-stake` (auto). Else: `stake` for the 3-path menu (recommend KYA path for AWP-less users) |
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
