# evm-track

A Rust port of an EVM chain tracker focused on BSC (Binance Smart Chain), using the Alloy stack. It listens to realtime events/blocks via WebSocket subscriptions, fetches historical ranges, decodes ABI events/functions, and supports modular actions (logging, JSON output, transfers, ownership changes, proxy upgrades, etc.).

## Features

- WebSocket subscriptions (logs/newHeads) with exponential backoff and bounded backfill
- Historical fetch for events and per-block processing
- ABI decoding (events: indexed + non-indexed; functions by selector)
- Data-driven: loads event/function signatures from `./data/*.json`
- Actions framework with built‑in actions:
  - Terminal logging (default)
  - JSON Lines output (`--json`)
  - ERC20 Transfer tracker (symbol/decimals via eth_call + cache, human‑readable amount)
  - OwnershipTransferred detector
  - Proxy upgrades with EIP‑1967 implementation/admin slot verification
  - Tornado (Deposit/Withdrawal) minimal detector
  - LargeTransfer alert (threshold-based Transfer alert)

## Requirements

- Rust (stable) and Cargo
- BSC (or other EVM) WebSocket endpoint (e.g., `wss://...`)

## Quick Start

From the project root (`evm-track`):

- Realtime events using WebSocket:
  - `cargo run -- track realtime --events --config ../EVM-trackooor/example_config.json`
- Realtime blocks (events + tx decoding):
  - `cargo run -- track realtime --blocks --config ../EVM-trackooor/example_config.json`
- Realtime pending transactions (pending mempool stream):
  - `cargo run -- track realtime --blocks --pending-blocks --config ../EVM-trackooor/example_config.json`
- Historical events (range with step):
  - `cargo run -- track historical events --config ../EVM-trackooor/example_config.json --from-block 100 --to-block 200 --step-blocks 100`
- Add JSON output:
  - Append `--json` to any of the above to print JSON Lines to stdout

Notes:
- The config file must contain a `rpcurl` (WebSocket URL) and `actions` with enabled addresses. See `../EVM-trackooor/example_config.json` for reference.
- Signatures are loaded from `./data/event_sigs.json` and `./data/func_sigs.json`. You can replace or extend these files as needed.
  - Override via CLI: add `--event-sigs <path>` and/or `--func-sigs <path>`.
  - Override via config: add `"event_sigs_path": "./path/event_sigs.json"`, `"func_sigs_path": "./path/func_sigs.json"`.

## CLI

- Global flags:
  - `-v, --verbose`: verbose console logs
  - `--json`: print JSON Lines to stdout
  - `--event-sigs <path>`: override event signatures JSON path
  - `--func-sigs <path>`: override function signatures JSON path
- Commands:
  - `track realtime events|blocks --config <path>`
  - `track historical events|blocks --config <path> --from-block <u64> [--to-block <u64>] [--step-blocks <u64>]`
  - `data event --abi <abi.json> [--output ./data/event_sigs.json]`

## Data Directory

- `./data/event_sigs.json`: map `topic0` hex to event ABI entries
- `./data/func_sigs.json`: map function selectors to signatures/ABIs
- `./data/blockscanners.json`: optional metadata for block scanners

## Data Commands

- Generate/merge event signatures from an ABI file:
  - `cargo run -- data event --abi ./path/to/contract.abi.json --output ./data/event_sigs.json`
  - Merges entries keyed by `topic0` into the output JSON.

## Config Example

You can set global signature paths and tune logging directly from the config JSON used with `--config`.

Example:

```
{
  "rpcurl": "wss://...",
  "event_sigs_path": "./custom/event_sigs.json",
  "func_sigs_path": "./custom/func_sigs.json",
  "actions": {
    "Logging": {
      "enabled": true,
      "addresses": {
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48": {}
      },
      "options": {
        "log-events": true,
        "log-transactions": true,
        "log-blocks": true,
        "enable-terminal-logs": true,
        "enable-discord-logs": false,
        "discord-webhook-url": "https://discord.com/api/webhooks/..."
      }
    },
    "TornadoCash": {
      "enabled": true,
      "addresses": {
        "0x910Cbd523D972eb0a6f4cAe4618aD62622b39DbF": { "name": "Torando.Cash 10 ETH" }
      },
      "options": {
        "output-filepath": "./tornadoCash_out.txt"
      }
    },
    "LargeTransfer": {
      "enabled": true,
      "addresses": {
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48": {}
      },
      "options": {
        "min-amount": "100000",          
        "decimals-default": 18            
      }
    }
  }
}
```

## Actions

Actions are pluggable. The following are built in by default:
- Logging (terminal)
- JSON Lines output (enable with `--json`)
- Transfer: fetches token symbol/decimals via `eth_call` and prints human‑readable amounts
- Ownership: detects OwnershipTransferred events
- Proxy: verifies implementation/admin via EIP‑1967 slots
- Tornado (sample): prints Deposit/Withdrawal detections
- LargeTransfer: alerts when Transfer amount exceeds a threshold

To extend: add new modules in `src/actions/` and include them in `src/main.rs` via the `ActionSet`.

## Throttling

- Global throttle limit is controlled by `max-requests-per-second` in the config (0 disables).
- The throttle is applied before major RPC calls: subscriptions, `get_block_number`, `get_logs`, `get_transaction_*`, `get_code_at`, `get_storage_at`, `eth_call`.

## JSON Output Schema

- Event record extra fields:
  - `decode_ok`: true if event signature decoded; false if unknown topic0.
  - `decode_error`: omitted or `"unknown_topic0"` when undecodable.
- Transaction record extra fields:
  - `decode_ok`: present only if input selector exists; true if function signature decoded.
  - `decode_error`: omitted or `"unknown_selector"` when undecodable.
  - `receipt_logs`: array of `{ address, topics, data, log_index, removed? }` mapped from receipt logs.

## Build & Lint

- Build: `cargo build --release`
- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-targets -- -D warnings`

## CI (GitHub Actions)

This repo includes a minimal CI that runs fmt, clippy, and build on push/PR to `main`.

## License

Proprietary or as per your project requirements.
