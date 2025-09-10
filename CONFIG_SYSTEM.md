# 统一配置加载和验证系统

## 🚀 功能特性

### 1. 统一配置加载器 (ConfigLoader)
- **多格式支持**: 自动检测并支持 JSON 和 TOML 配置文件
- **智能解析**: 根据文件扩展名选择合适的解析器，无扩展名时自动尝试两种格式
- **动作特定配置**: 支持为特定动作加载独立的配置文件
- **灵活文件命名**: 支持多种配置文件命名模式

### 2. 配置验证器 (ConfigValidator)
- **主配置验证**: 验证RPC URL格式和动作配置完整性
- **地址格式验证**: 自动验证所有以太坊地址格式
- **动作特定验证**: 针对不同动作类型的专门验证规则
- **配置完整性检查**: 跨动作依赖关系验证

### 3. 错误处理和日志
- **详细错误信息**: 明确指出配置问题的具体位置和原因
- **结构化日志**: 使用 tracing 提供调试和信息日志
- **优雅降级**: 合理的默认值和警告机制

## 📋 使用方法

### 基本配置加载
```rust
use evm_track::config::load_and_validate_config;
use std::path::Path;

let config = load_and_validate_config(Path::new("config.json"))?;
```

### 动作特定配置加载
```rust
use evm_track::config::ConfigLoader;

let action_config = ConfigLoader::load_action_config::<MyActionConfig>(
    "transfer", 
    Some(Path::new("./configs/"))
)?;
```

### 手动验证
```rust
use evm_track::config::ConfigValidator;

// 验证主配置
ConfigValidator::validate_main_config(&config)?;

// 验证配置完整性
ConfigValidator::validate_config_integrity(&config)?;

// 验证特定动作
ConfigValidator::validate_action_config("transfer", &action_config)?;
```

## 🔧 配置文件格式

### JSON 格式 (推荐)
```json
{
  "rpcurl": "https://rpc.ankr.com/eth",
  "max-requests-per-second": 10,
  "event_sigs_path": "./data/event_sigs.json",
  "func_sigs_path": "./data/func_sigs.json",
  "actions": {
    "transfer": {
      "enabled": true,
      "addresses": {
        "0xA0b86a33E6418de4bE4C96D4c3c1EbcDFf0aA78E": {},
        "0xdAC17F958D2ee523a2206206994597C13D831ec7": {}
      },
      "options": {
        "min_amount": "1000000000000000000"
      }
    }
  }
}
```

### TOML 格式
```toml
rpcurl = "https://rpc.ankr.com/eth"
max-requests-per-second = 15

[actions.transfer]
enabled = true
[actions.transfer.options]
min_amount = "500000000000000000"
[actions.transfer.addresses]
"0xA0b86a33E6418de4bE4C96D4c3c1EbcDFf0aA78E" = {}
```

## ⚡ 验证规则

### RPC URL 验证
- 必须以 `http://`, `https://`, `ws://`, 或 `wss://` 开头
- 不能为空字符串

### 地址验证
- 所有地址必须是有效的以太坊地址格式
- 自动验证 `addresses` 映射中的所有键

### 动作特定验证
- **transfer**: `min_amount` 必须是有效数字
- **large_transfer**: `threshold` 必须是有效的正数（支持大数值）
- **ownership**: 至少需要一个合约地址
- **deployment**: 可以没有地址（监控所有部署）

### 完整性验证
- 检查动作间的依赖关系
- 例如：启用 `large_transfer` 时建议同时启用 `transfer`

## 🎯 错误处理

### 常见错误类型
1. **配置文件不存在**: `Configuration file not found`
2. **格式错误**: `Invalid JSON/TOML in file`
3. **地址格式错误**: `Invalid address 'xxx' in action 'yyy'`
4. **参数验证失败**: `Invalid threshold in large_transfer config`

### 错误信息特点
- 明确指出问题的具体位置
- 提供修复建议
- 包含完整的错误路径

## 🔄 迁移指南

### 从旧系统迁移
所有使用 `config::load_config()` 的地方已自动更新为 `config::load_and_validate_config()`：

- `src/main.rs`: HistoryTxScan 命令
- `src/commands/track.rs`: Realtime 和 Historical 命令
- `src/commands/init_scan_cmd.rs`: InitScan 命令

### 性能改进
- **自动验证**: 配置加载时自动进行全面验证
- **早期错误检测**: 启动时发现配置问题，避免运行时错误
- **多格式支持**: 无需手动选择解析器

## 📊 测试覆盖

### 测试用例
1. **基本功能测试**: `examples/test_config_system.rs`
2. **TOML 支持测试**: `examples/test_toml_config.rs`
3. **错误处理测试**: 无效配置文件和格式错误

### 运行测试
```bash
cargo run --example test_config_system
cargo run --example test_toml_config
```

## 🎉 架构优势

### 1. 统一性
- 所有配置加载使用相同的接口和验证流程
- 消除了代码重复和不一致的处理方式

### 2. 可扩展性
- 易于添加新的配置验证规则
- 支持新的配置文件格式
- 模块化的验证器设计

### 3. 健壮性
- 全面的错误处理和验证
- 清晰的错误信息和修复建议
- 配置完整性检查防止配置冲突

### 4. 开发者友好
- 丰富的日志输出便于调试
- 灵活的配置文件命名支持
- 向后兼容现有配置文件

这个统一配置系统显著提高了项目的配置管理质量，为后续的功能扩展提供了坚实的基础。
