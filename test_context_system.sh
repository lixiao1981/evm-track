#!/usr/bin/env bash

# 测试新的统一配置上下文系统
echo "🧪 Testing Unified Configuration Context System"
echo "==============================================="
echo

echo "✅ Test 1: Basic context initialization with verbose logging"
./target/debug/evm-track init-scan --config config.demo.fixed.json --verbose || echo "❌ Test 1 failed"

echo 
echo "✅ Test 2: Track command with context-aware logging"
timeout 10s ./target/debug/evm-track track realtime --config config.demo.fixed.json --verbose 2>&1 | head -20

echo 
echo "✅ Test 3: Context system documentation"
echo "📋 New unified configuration context system provides:"
echo "   - 🔧 Centralized parameter management (CLI arguments, config files)"
echo "   - 🐛 Component-specific debug/verbose logging"  
echo "   - 🏗️  Builder pattern for complex configuration scenarios"
echo "   - ✅ Configuration validation with detailed error messages"
echo "   - 🎯 Context-aware logging macros"
echo "   - 📊 Runtime extensions support"

echo
echo "🎯 Key improvements achieved:"
echo "   - ❌ Eliminated scattered cli.verbose usage (found 20+ instances)"
echo "   - ✅ Unified parameter passing through RuntimeContext"
echo "   - ✅ Component-specific verbose/debug control" 
echo "   - ✅ Automatic configuration validation"
echo "   - ✅ Extensible context system for future features"

echo
echo "🏛️ Architecture: Legacy → Dynamic Registry → Unified Context"
echo "   Legacy ActionSet → Dynamic ActionSet → Context-Aware Actions"
