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

## Requirements

- Rust (stable) and Cargo
- BSC (or other EVM) WebSocket endpoint (e.g., `wss://...`)

## Quick Start

From the project root (`evm-track`):

- Realtime events using WebSocket:
  - `cargo run -- track realtime events --config ../EVM-trackooor/example_config.json`
- Realtime blocks (events + tx decoding):
  - `cargo run -- track realtime blocks --config ../EVM-trackooor/example_config.json`
- Historical events (range with step):
  - `cargo run -- track historical events --config ../EVM-trackooor/example_config.json --from-block 100 --to-block 200 --step-blocks 100`
- Add JSON output:
  - Append `--json` to any of the above to print JSON Lines to stdout

Notes:
- The config file must contain a `rpcurl` (WebSocket URL) and `actions` with enabled addresses. See `../EVM-trackooor/example_config.json` for reference.
- Signatures are loaded from `./data/event_sigs.json` and `./data/func_sigs.json`. You can replace or extend these files as needed.

## CLI

- Global flags:
  - `-v, --verbose`: verbose console logs
  - `--json`: print JSON Lines to stdout
- Commands:
  - `track realtime events|blocks --config <path>`
  - `track historical events|blocks --config <path> --from-block <u64> [--to-block <u64>] [--step-blocks <u64>]`

## Data Directory

- `./data/event_sigs.json`: map `topic0` hex to event ABI entries
- `./data/func_sigs.json`: map function selectors to signatures/ABIs
- `./data/blockscanners.json`: optional metadata for block scanners

## Actions

Actions are pluggable. The following are built in by default:
- Logging (terminal)
- JSON Lines output (enable with `--json`)
- Transfer: fetches token symbol/decimals via `eth_call` and prints human‑readable amounts
- Ownership: detects OwnershipTransferred events
- Proxy: verifies implementation/admin via EIP‑1967 slots
- Tornado (sample): prints Deposit/Withdrawal detections

To extend: add new modules in `src/actions/` and include them in `src/main.rs` via the `ActionSet`.

## Build & Lint

- Build: `cargo build --release`
- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-targets -- -D warnings`

## CI (GitHub Actions)

This repo includes a minimal CI that runs fmt, clippy, and build on push/PR to `main`.

## License

Proprietary or as per your project requirements.
