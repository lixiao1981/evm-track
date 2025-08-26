# evm-track

一个基于 Rust + Alloy 的 EVM 链追踪器，当前偏向 BSC（Binance Smart Chain）使用场景。支持 WebSocket 订阅实时事件/区块、历史区间抓取、ABI 事件/函数解码，以及可插拔的 Actions（日志、JSON 输出、转账追踪、所有权变更、代理合约升级、部署扫描、Tornado 检测、大额转账告警等）。

## 功能特性

- 实时订阅：`logs` / `newHeads` WebSocket 订阅，支持指数退避与有限回填。
- 历史回溯：按区间、按步长抓取事件或逐区块处理。
- ABI 解码：支持事件（indexed + non-indexed）与函数（按 selector）。
- 数据驱动：从 `./data/*.json` 载入事件/函数签名，可自定义覆盖路径。
- Actions 框架（可插拔）：
  - 终端日志（默认）。
  - JSON Lines 输出（通过 `--json` 开启）。
  - ERC20 Transfer 追踪（`eth_call` 读取 symbol/decimals + 缓存，人类可读数额）。
  - OwnershipTransferred 检测。
  - 代理升级检测（校验 EIP‑1967 implementation/admin 槽位）。
  - 合约部署扫描（字节码哈希、大小、EIP‑1167 极简代理指纹、EIP‑1967 常量片段）。
  - Tornado（Deposit/Withdrawal）简化检测。
  - LargeTransfer（大额转账告警，基于阈值）。
- 全局节流：按秒限制最大 RPC 调用次数，避免触发速率限制。

## 运行要求

- Rust（stable）与 Cargo。
- EVM WebSocket 节点（例如 `wss://...`）。

## 目录结构

```
src/
  abi.rs           # 事件/函数解码
  actions/         # 可插拔动作
  cli.rs           # 命令行定义
  config.rs        # 配置解析
  data_cmd.rs      # 数据工具命令
  provider.rs      # Provider 连接
  runtime/         # 实时/历史运行逻辑
  throttle.rs      # 全局节流
  main.rs
data/
  event_sigs.json  # 事件签名映射（可自备/覆盖）
  func_sigs.json   # 函数签名映射（可自备/覆盖）
```

## 快速开始

以下命令在项目根 `evm-track/` 下执行。

- 实时事件订阅：
  - `cargo run -- track realtime --events --config ../EVM-trackooor/example_config.json`
- 实时区块（事件 + 交易解码）：
  - `cargo run -- track realtime --blocks --config ../EVM-trackooor/example_config.json`
- 实时待打包交易（mempool 流）：
  - `cargo run -- track realtime --blocks --pending-blocks --config ../EVM-trackooor/example_config.json`
- 历史事件（区间 + 步长）：
  - `cargo run -- track historical events --config ../EVM-trackooor/example_config.json --from-block 100 --to-block 200 --step-blocks 100`
- 输出 JSON 行：
  - 在任一命令后追加 `--json` 将以 JSON Lines 形式打印到 stdout。

### InitScan（历史初始化扫描）

此子命令按给定区块号范围，遍历每个区块内的交易，找到 CREATE（`to == null`）类型的部署交易，取回执中的 `contractAddress` 并调用 Initscan 逻辑尝试初始化（`eth_call` + `trace_call`/`stateDiff` + 随机 selector 复检）。

- 节点要求：需支持 WebSocket 与 `trace_call`（如 Erigon、Nethermind、OpenEthereum）。
- 示例配置：`./config.example.initscan.json`，在 `actions.Initscan` 中提供：
  - `from-address`
  - `check-addresses`
  - `function-signature-calldata`
  - 可选：`initializable-contracts-filepath` 与 `init-known-contracts-frequency` 以持久化与周期重试。
- 运行示例：
  - `cargo run -- init-scan --config ./config.example.initscan.json --from-block 10000000 --to-block 10001000`

运行后会：
- 遍历 [from, to] 区间，拉取包含完整交易的区块，识别 CREATE 交易；
- 获取回执 `contractAddress`，对每个合约按配置的 calldata 尝试初始化；
- 若命中（存在关键地址的 `stateDiff` 且通过随机 selector 复检），在终端或 webhook 输出，若配置了持久化文件也会写入。

注意：
- 配置文件需包含 `rpcurl`（WebSocket URL）与 `actions` 中至少一个 `enabled: true` 的地址。可参考 `../EVM-trackooor/example_config.json`。
- 事件/函数签名默认从 `./data/event_sigs.json` 与 `./data/func_sigs.json` 读取：
  - CLI 覆盖：`--event-sigs <path>`、`--func-sigs <path>`。
  - 配置覆盖：`"event_sigs_path": "./path/event_sigs.json"`、`"func_sigs_path": "./path/func_sigs.json"`。
- 全局节流：配置顶层 `"max-requests-per-second": <u32>`（0 表示关闭）。

## CLI 说明

全局参数：
- `-v, --verbose`：更详细的控制台日志。
- `--json`：打印 JSON Lines 到 stdout。
- `--event-sigs <path>`：覆盖事件签名 JSON 路径。
- `--func-sigs <path>`：覆盖函数签名 JSON 路径。

命令：
- `track realtime events|blocks --config <path>`：实时追踪事件或区块。
- `--pending-hashes-only`：在 realtime 模式下，强制使用“待打包交易哈希订阅”。当某些节点 full-pending 返回的字段缺失（例如缺 `from`）导致反序列化错误时，建议加此开关。
- `track historical events|blocks --config <path> --from-block <u64> [--to-block <u64>] [--step-blocks <u64>]`：历史区间抓取。
- `data event --abi <abi.json> [--output ./data/event_sigs.json]`：从 ABI 文件合并/生成事件签名数据。

## 配置文件详解

示例：

```
{
  "rpcurl": "wss://...",
  "event_sigs_path": "./custom/event_sigs.json",
  "func_sigs_path": "./custom/func_sigs.json",
  "max-requests-per-second": 20,
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

要点说明：
- `actions.*.enabled`：是否启用该 Action。
- `actions.*.addresses`：本 Action 关注的合约地址集合（影响过滤与处理范围）。
- `Logging.options`：开关终端/Discord 输出与日志粒度。
- `TornadoCash.options.output-filepath`：可选，将检测结果追加写入文件。
- `LargeTransfer.options.min-amount`：人类可读阈值（字符串，如 `"100000"` 或 `"100000.5"`）。
- `LargeTransfer.options.decimals-default`：默认小数位（18），用于未知代币阈值换算。
- `max-requests-per-second`：节流上限（每秒允许的 RPC 请求数，0 表示关闭）。

## JSON 输出格式（`--json`）

事件（kind = `"event"`）字段：
- `address`：事件合约地址。
- `tx_hash`：交易哈希（可选）。
- `block_number`：区块高度（可选）。
- `name`：事件名（解码成功时）。
- `decode_ok`：是否成功按 `topic0` 解码。
- `decode_error`：未匹配签名时为 `"unknown_topic0"`（无错误省略）。
- `fields`：字段数组，形如 `[(name, value_string)]`。
- `tx_index`、`log_index`、`topics`、`removed`。

交易（kind = `"tx"`）字段：
- `hash`、`from`、`to`。
- `func`：函数名（解码成功时）。
- `decode_ok`：仅当存在 selector 时出现；可解析到函数签名时为 `true`。
- `decode_error`：当存在 selector 但未匹配到签名时为 `"unknown_selector"`（否则省略）。
- `gas`、`gas_price`、`effective_gas_price`、`status`、`gas_used`、`cumulative_gas_used`。
- `block_number`、`tx_index`、`contract_address`。
- `receipt_logs`：回执日志数组，元素为 `{ address, topics, data, log_index, removed? }`。

区块（kind = `"block"`）字段：
- `number`。

## Actions 详解

- Logging：打印简要事件/交易/区块信息，可配置终端/Discord 开关。
- JsonLog：将事件/交易/区块以 JSON 行输出到 stdout。
- Transfer：识别 `Transfer` 事件，`eth_call` 读取 `decimals()/symbol()` 并缓存，输出人类可读数值。
- Ownership：识别 `OwnershipTransferred`（或相近）事件，打印前后所有者。
- Proxy：识别升级/管理员变更事件，读取 EIP‑1967 槽位校验链上实现/管理员。
- Deployment：检测新部署合约（回执中 `contractAddress`），拉取运行时字节码、计算 keccak、判断 EIP‑1167 极简代理指纹、是否包含 EIP‑1967 常量片段等。
- Tornado：简单的 `Deposit`/`Withdrawal` 检测，可选写入指定文件。
- LargeTransfer：当 `Transfer` 金额 ≥ 阈值（按 `decimals-default` 转换）时输出告警行。

扩展 Action：在 `src/actions/` 下新增模块，并在 `src/actions/mod.rs` 注册，在 `src/main.rs` 创建 `ActionSet` 时加入实例即可。

## 节流（Throttle）

- 配置项：`"max-requests-per-second": <u32>`，默认 0（关闭）。
- 作用范围：
  - 订阅与回填：`subscribe_*`、`get_block_number`、`get_logs`。
  - 交易：`get_transaction_by_hash`、`get_transaction_receipt`。
  - 动作：`get_code_at`、`get_storage_at`、`eth_call`。
- 实现方式：令牌桶（每秒补满到上限）。

## 运行模式说明

- 实时 events：优先使用订阅，失败自动退回轮询；订阅中断后进行有限回填并重试（指数退避）。
- 实时 blocks：订阅新区块并在该区块过滤日志、解码交易，失败退回轮询并回填。
- 实时 pending：优先 `fullPendingTransactions`；若节点不兼容（如缺字段导致订阅项反序列化失败），请添加 `--pending-hashes-only` 强制走 `pendingTransaction` 哈希流。

## BSC WebSocket 节点示例

- NodeReal（需注册获取 API Key）：`wss://bsc-mainnet.nodereal.io/ws/v1/<YOUR_API_KEY>`
- QuickNode（需注册）：`wss://<YOUR_WORKSPACE_NAME>.quiet-late-sun.bsc.quiknode.pro/<YOUR_API_KEY>/`
- Ankr（需注册）：`wss://rpc.ankr.com/bsc/<YOUR_API_KEY>`
- GetBlock（需注册）：`wss://bsc.getblock.io/mainnet/?api_key=<YOUR_API_KEY>`
- Chainstack（需注册）：`wss://nd-<PROJECT>-<ID>.chainstacklabs.com/ws`

注意：大多数服务商的 WebSocket 端点需要 API Key，请在配置中替换为你的实际密钥。部分历史上公开的 BSC WS 节点稳定性较差，生产环境建议使用有 SLA 的服务商或自建全节点。
- 历史 events/blocks：按区间迭代抓取，`blocks` 模式中额外按交易解码与回执读取。

## 数据工具

从 ABI 文件生成/合并事件签名：

```
cargo run -- data event --abi ./path/to/contract.abi.json --output ./data/event_sigs.json
```

该命令将 ABI 中的事件按 `keccak256("Name(type1,type2,...)")` 计算得到的 `topic0` 写入到目标 JSON（键冲突时覆盖）。

## 构建与质量

- 构建：`cargo build --release`
- 格式化：`cargo fmt --all`
- Lint：`cargo clippy --all-targets -- -D warnings`

CI（GitHub Actions）：

- `.github/workflows/ci.yml` 包含基础的 fmt/clippy/build 流程。

## 常见问题（FAQ）

- 无日志输出？检查 `actions.*.enabled` 与 `addresses` 是否配置；或临时移除地址限制以观察全网事件。
- 订阅失败或断开？程序会自动退回轮询并指数退避；检查节点是否支持 WS 订阅，或考虑更换节点。
- 速率限制？配置 `max-requests-per-second`；减少 `step-blocks`；必要时多节点负载均衡。
- JSON 太冗长？关闭 `--json` 或仅启用需要的 Actions；通过地址过滤减少数据量。

## 许可

按你的项目要求（Proprietary 或自定义）。
