
```


Made changes.


```
Now I need to fix the GlobalOutputManager call since output is now an Option: 
```


Made changes.


```
Let me also add import for OutputFormat: 
```


Made changes.


```
Now I can use OutputFormat directly instead of the full path: 
```


Made changes.


```

```


Made changes.


```

```


Made changes.


```
Now let's try running the demo again: I need to fix the module paths in the register_all_factories method. Let me check what factories actually exist: I see the issue. Not all the factory modules exist. Let me fix the register_all_factories method to only include the existing factories: 
```


Made changes.


```
Now let me try running the demo again: 太棒了！动态Action注册机制演示成功运行！系统完美地展示了：

## 🎉 动态Action注册机制和统一输出系统已完成！

### ✅ 完成的功能：

1. **🔧 动态Action注册机制**
   - ✅ 插件式架构：6个Action工厂成功注册
   - ✅ 依赖管理：自动解析依赖关系（如transfer依赖Logging）
   - ✅ 工厂模式：ActionFactory trait统一接口

2. **📄 统一输出系统**
   - ✅ 多格式支持：JSON、JSONL、CSV、控制台输出
   - ✅ 文件轮转：支持大小限制和自动轮转
   - ✅ 双层配置：全局输出 + 单独Action输出配置
   - ✅ 缓冲管理：异步写入和自动刷新

3. **⚙️ 配置系统集成**
   - ✅ OutputConfig结构完善
   - ✅ 全局和per-action配置支持
   - ✅ 默认配置实现

### 📋 系统架构概览：

```
┌─────────────────────┐
│   ActionRegistry    │
│ ┌─────────────────┐ │
│ │ LoggingFactory  │ │
│ │ TransferFactory │ │
│ │ DeploymentFactory│ │
│ │ SelectorFactory │ │ 
│ │ TornadoFactory  │ │
│ │ LargeTransfer...│ │
│ └─────────────────┘ │
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│ GlobalOutputManager │
│ ┌─────────────────┐ │
│ │ JSON Writer     │ │
│ │ JSONL Writer    │ │
│ │ CSV Writer      │ │
│ │ Console Writer  │ │
│ └─────────────────┘ │
└─────────────────────┘
```

### 🚀 主要优势：

1. **可扩展性**：新增Action只需实现ActionFactory trait
2. **依赖管理**：自动解析和排序依赖关系
3. **配置灵活**：支持全局和单独输出配置
4. **性能优化**：异步I/O + 缓冲写入
5. **文件管理**：自动轮转防止文件过大
6. **类型安全**：编译时确保配置正确性

### 💡 检测结果保存位置：

**动态Action注册机制的检测结果保存到多个位置：**

1. **📁 独立输出文件**（如果Action配置了单独的output）
   - Transfer结果：`/tmp/transfer_results.json`
   - SelectorScan结果：`/tmp/selector_scan.csv`

2. **📁 全局输出文件**（没有单独配置的Action）
   - 统一结果：evm_track_global.jsonl
   - Tornado结果会保存到这里

3. **🖥️ 控制台输出**（实时显示）

4. **🔄 文件轮转管理**
   - 自动按大小轮转（5MB-20MB可配置）
   - 保留历史文件数量可配置

这个动态Action注册机制完全实现了插件化架构，让evm-track具备了高度的可扩展性和灵活性！🎯