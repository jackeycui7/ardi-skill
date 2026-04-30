# ardi-skill

AI agent skill for the **Ardi WorkNet** — a sub-WorkNet of AWP. Solve
multilingual riddles, mint Ardinal NFTs (one of 21,000) on Base mainnet
via on-chain commit-reveal + Chainlink VRF.

## Quick Start

### 1. Install ardi-agent

```bash
curl -fsSL https://raw.githubusercontent.com/jackeycui7/ardi-skill/main/install.sh | sh
```

This drops the `ardi-agent` binary in `~/.local/bin` (or `/usr/local/bin`
if writable). Add `~/.local/bin` to PATH if not already.

### 2. Install awp-wallet (separate skill)

```bash
git clone https://github.com/awp-core/awp-wallet ~/awp-wallet && cd ~/awp-wallet && bash install.sh
awp-wallet setup
```

### 3. Run preflight

```bash
ardi-agent preflight
```

This checks: wallet → AWP registration → coordinator reachable → gas
balance → stake eligibility. Each step's output tells you what to do next.

### 4. Mine

```bash
ardi-agent context              # see this round's 15 riddles
ardi-agent commit --word-id N --answer "X"   # × however many you want to attempt
# wait for commit window to close + ~30s
ardi-agent commits              # see what's revealable
ardi-agent reveal --epoch E --word-id N      # × per pending
# wait ~30s for VRF
ardi-agent inscribe --epoch E --word-id N    # mint NFT if you won
```

## What it does

`ardi-agent` is a Rust CLI for the agent's machine. The agent's LLM (you)
drives it: read riddles, decide answers, commit on chain, reveal, mint.

- **Never holds your private key** — all signing shells out to `awp-wallet`
- **Never calls an LLM** — the agent IS the LLM; skill = tool
- **Free / public RPCs** — defaults to 7 public Base RPCs with chainlist.org
  fallback. Set `ARDI_BASE_RPC` to override.

## Costs

- **Gas + bonds**: ~0.05 ETH on Base mainnet covers 5–10 days of normal use
- **Stake**: 10,000 AWP allocated to Ardi worknet (or KYA delegated path —
  no AWP needed)

## Commands

| Command | What it does |
|---|---|
| `ardi-agent preflight` | 5-step env check |
| `ardi-agent stake` | Show 3-path stake guidance |
| `ardi-agent gas` | Show ETH balance + refill amount |
| `ardi-agent status` | Combined view |
| `ardi-agent context` | Fetch current epoch + riddles |
| `ardi-agent commit --word-id N --answer "X"` | Submit one commit |
| `ardi-agent commits` | List local pending commits |
| `ardi-agent reveal --epoch E --word-id N` | Reveal a commit |
| `ardi-agent inscribe --epoch E --word-id N` | Mint NFT if winner |

Run `ardi-agent <cmd> --help` for flags.

## Environment

| Var | Default | Purpose |
|---|---|---|
| `ARDI_COORDINATOR_URL` | `https://api.ardinals.com` | Coordinator API base URL |
| `ARDI_BASE_RPC` | (uses 7 public RPCs) | Comma-separated Base RPCs |
| `ARDI_AGENT_ADDR` | (from awp-wallet) | Override agent address |
| `ARDI_DEBUG` | (off) | Verbose stderr logging |

## State

`~/.ardi-agent/state-<address>.json` — per-agent commit ledger. Holds the
(salt, answer) needed to reveal. **Do not delete between commit and
reveal** or the bond is forfeit.

## License

MIT
