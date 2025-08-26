# Gemini Context

This file provides context and instructions for the Gemini agent.

## Project Overview

`evm-track` is a Rust and Alloy based EVM chain tracker, with a focus on Binance Smart Chain (BSC).

### Key Features:
- Real-time event/block tracking via WebSocket.
- Historical data fetching.
- ABI decoding for events and functions.
- A pluggable "Actions" system for tasks like logging, transfer tracking, and security alerts (e.g., Tornado Cash detection).
- Global RPC call throttling.

### Technology Stack:
- Rust
- Alloy (Rust library for Ethereum)

### How to Run:
The project is a command-line tool run with `cargo run`. It has subcommands like `track` (for real-time/historical tracking) and `init-scan`. It requires a configuration JSON file and a WebSocket endpoint for an EVM node.
