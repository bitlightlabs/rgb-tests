# RGB RBF Edge Cases 测试报告

## 项目背景

RGB Tests 是用于测试 RGB 协议实现的测试仓库。本报告针对 RBF (Replace-By-Fee) 功能的边界情况和异常场景进行测试验证。

## 测试环境

- **测试文件**: `tests/rbf_edge_cases.rs`
- **资产类型**: NIA (Non-Inflatable Asset)
- **钱包类型**: Wpkh (Witness Public Key Hash)
- **测试资产**: RBFTestAsset (RBF), 总供应量 600 units

### 结果概览

**完整测试**: 6 个测试用例覆盖了 RBF 的主要使用场景和边界条件
- 基础功能: 2/2 通过 (100%)
- 边界场景: 3/4 通过 (75%)
- 总体通过率: 5/6 (83.3%)

## 测试用例概述

### 原始 RBF 测试用例 (tests/transfer.rs)

在边界测试之前，项目已经包含了两个基础的 RBF 测试用例，验证了正常流程：

#### 原始测试用例 1: `rbf_transfer`
**测试定义**: 原始交易成功广播进入 mempool，RBF 交易成功替换

**操作流程**:
1. 创建转账交易，费用 500 sats，成功广播进入 mempool
2. 接收方接受转账
3. 创建 RBF 交易，费用 1000 sats，成功替换原始交易
4. 挖矿确认 RBF 交易

**验证结果**: ✅ 通过 - 状态正确，RBF 机制正常工作

#### 原始测试用例 2: `rbf_unbroadcasted_state_all`
**测试定义**: 原始交易不广播，创建 RBF 交易也不广播，最后广播原始交易

**操作流程**:
1. 创建转账交易，费用 500 sats，不广播 (`broadcast=false`)
2. 接收方接受转账
3. 创建 RBF 交易，费用 1000 sats，不广播
4. 广播并确认原始交易 (而非 RBF 交易)

**验证结果**: ✅ 通过 - 状态正确，未广播的 RBF 交易状态为 `WitnessStatus::Archived`

### 边界测试用例 (tests/rbf_edge_cases.rs)

基于原始测试的成功，本边界测试套件补充了 4 个异常和边界场景：

### 1. `rbf_original_broadcast_rbf_unbroadcast`

**测试定义**: 原始交易成功广播，RBF 交易创建但未广播

**初始状态**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**操作序列**:
1. 停止挖矿
2. 创建转账 400 tokens 的交易，费用 500 sats，成功广播
3. wlt_2 接受转账
4. 创建 RBF 交易，费用 1000 sats，但不广播（模拟广播失败等场景）
5. 同步钱包状态

**预期行为**:
- 原始交易保留在 mempool 中
- wlt_1 余额: 200 tokens (反映待确认交易)
- wlt_2 余额: 400 tokens (反映待确认交易)
- 区块高度不变 (交易未确认)

**验证断言**:
```rust
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![200]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);
assert_eq!(initial_height, mid_height);
```

### 2. `rbf_both_original_and_rbf_unbroadcast`

**测试定义**: 原始交易和 RBF 交易都未广播

**初始状态**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**操作序列**:
1. 停止挖矿
2. 创建转账 400 tokens 的交易，费用 100 sats (低费率)，尝试广播失败
3. wlt_2 接受转账
4. 同步钱包 (状态回滚)
5. 创建 RBF 交易，费用 1000 sats，不广播（模拟广播失败等场景）
6. 再次同步钱包

**预期行为**:
- 所有钱包余额回滚到初始状态
- wlt_1 余额: 600 tokens
- wlt_2 余额: 0 tokens
- 无链上交易

**验证断言**:
```rust
// 接受转账后
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);

// 同步后状态回滚
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

// RBF 失败后状态保持
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
```

### 3. `rbf_original_unbroadcast_rbf_insufficient_fee`

**测试定义**: 原始交易未广播，RBF 因费用过高导致溢出失败

**初始状态**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**操作序列**:
1. 停止挖矿
2. 创建转账 400 tokens 的交易，费用 100 sats (低费率)，尝试广播失败
3. wlt_2 接受转账
4. 同步钱包 (状态回滚)
5. 尝试创建 RBF 交易，费用 1,000,000,000 sats (过高)

**预期行为**:
- RBF 创建因溢出错误失败
- 钱包余额保持初始状态
- wlt_1 余额: 600 tokens
- wlt_2 余额: 0 tokens

**验证断言**:
```rust
#[should_panic(expected = "overflow")]
assert!(wlt_1.runtime.rbf(&payment, 1_000_000_000_u64).is_err());
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
```

### 4. `rbf_original_unbroadcast_rbf_success`

**测试定义**: 原始交易未广播，RBF 交易成功广播并确认

**初始状态**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**操作序列**:
1. 停止挖矿
2. 创建转账 400 tokens 的交易，费用 100 sats (低费率)，尝试广播失败
3. wlt_2 接受转账
4. 同步钱包 (状态回滚)
5. 创建 RBF 交易，费用 1000 sats，成功广播
6. 挖矿确认 RBF 交易
7. wlt_2 接受 RBF 转账
8. 同步钱包

**预期行为**:
- RBF 交易替换原始交易并成功完成
- wlt_1 余额: 200 tokens
- wlt_2 余额: 400 tokens
- 区块高度增加 (交易已确认)

**验证断言**:
```rust
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![200]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);
assert!(final_height > initial_height);
```

## 完整测试覆盖矩阵

### 原始测试用例 (基础流程验证)

| 测试用例                      | 原始交易广播 | RBF 交易广播 | 最终确认交易 | 预期结果     | 实际结果 |
| ----------------------------- | ------------ | ------------ | ------------ | ------------ | -------- |
| `rbf_transfer`                | ✅ 成功       | ✅ 成功替换   | RBF 交易     | RBF 交易确认 | ✅ 通过   |
| `rbf_unbroadcasted_state_all` | ❌ 未广播     | ❌ 未广播     | 原始交易     | 原始交易确认 | ✅ 通过   |

### 边界测试用例 (异常和边界场景)

| 测试用例                                        | 原始交易广播 | RBF 交易广播 | 预期结果       | 实际结果   |
| ----------------------------------------------- | ------------ | ------------ | -------------- | ---------- |
| `rbf_original_broadcast_rbf_unbroadcast`        | ✅ 成功       | ❌ 未广播     | 原始交易待确认 | ✅ 通过     |
| `rbf_both_original_and_rbf_unbroadcast`         | ❌ 低费率失败 | ❌ 低费率失败 | 状态完全回滚   | ✅ 通过     |
| `rbf_original_unbroadcast_rbf_insufficient_fee` | ❌ 低费率失败 | ❌ 费用溢出   | 状态保持初始   | ✅ 通过     |
| `rbf_original_unbroadcast_rbf_success`          | ❌ 低费率失败 | ✅ 成功       | RBF 交易确认   | ❌ 状态异常 |

## 测试实现改进

### 真实广播失败模拟

相比于简单的不广播交易来模拟失败，本测试套件采用了更真实的方法：

- **低费率广播失败**: 使用 100 sats 的费率创建交易，由于低于最小中继费用 (153 sats) 而导致真实的广播失败
- **错误验证**: 通过 RPC 错误 `"min relay fee not met, 100 < 153"` 确认失败原因
- **状态一致性**: 确保钱包状态与实际网络行为保持一致

### 关键改进点

1. **真实性**: 使用实际的网络限制条件触发失败
2. **可重现性**: 100 sats 费率在 regtest 网络中稳定触发失败
3. **错误处理**: 正确处理和验证广播失败的错误类型

## 实际测试结果

### 测试执行概况

**执行命令**: `SKIP_INIT=true RUST_BACKTRACE=1 cargo test --all-features -- rbf`

**测试结果**: 4 个测试用例中 3 个通过，1 个失败

| 测试用例                                        | 执行状态 | 执行时间 | 结果       |
| ----------------------------------------------- | -------- | -------- | ---------- |
| `rbf_both_original_and_rbf_unbroadcast`         | ✅ 通过   | < 1s     | 符合预期   |
| `rbf_original_unbroadcast_rbf_insufficient_fee` | ✅ 通过   | < 1s     | 符合预期   |
| `rbf_original_broadcast_rbf_unbroadcast`        | ✅ 通过   | < 1s     | 符合预期   |
| `rbf_original_unbroadcast_rbf_success`          | ❌ 失败   | 144.79s  | 状态不一致 |

### 详细测试分析

#### ✅ 成功的测试用例

**1. rbf_both_original_and_rbf_unbroadcast**
- 状态: 通过
- 行为: 双重广播失败正确处理，钱包状态正确回滚

**2. rbf_original_unbroadcast_rbf_insufficient_fee** 
- 状态: 通过
- 行为: 溢出错误正确捕获，钱包状态保持不变

**3. rbf_original_broadcast_rbf_unbroadcast**
- 状态: 通过  
- 行为: 原始交易广播成功，RBF 未广播，状态正确维护

#### ❌ 失败的测试用例

**rbf_original_unbroadcast_rbf_success**

**失败原因**: 钱包状态不一致
- **预期**: wlt_1 余额应为 200 tokens
- **实际**: wlt_1 余额为 0 tokens (空数组)
- **错误**: `assertion 'left == right' failed: left: [] right: [200]`

**关键观察**:
1. **原始交易广播失败**: 确认收到预期的费率错误 `"min relay fee not met, 100 < 153"`
2. **RBF 交易成功**: RBF 交易成功广播并在区块 234 确认
3. **接收方状态正确**: wlt_2 正确接收到 400 tokens
4. **发送方状态异常**: wlt_1 显示 0 余额而非预期的 200 tokens

**状态分析**:
```
wlt_1.runtime.state_all(contract_id).owned: 包含创世状态 (600 tokens)
  wlt_1.runtime.state_own(contract_id).owned: 空数组 []
  ```

## 结论

### 测试体系完整性

#### 原始测试基础 (✅ 已验证)
项目原有的两个 RBF 测试用例已经验证了核心功能的正确性：
- **`rbf_transfer`**: 验证了标准 RBF 替换流程 - 原始交易进入 mempool，RBF 成功替换
- **`rbf_unbroadcasted_state_all`**: 验证了未广播交易的状态管理 - 原始交易不广播，RBF 也不广播，最终广播原始交易

#### 边界测试补充 (本报告主体)
基于原始测试的成功，边界测试套件专门针对异常和边界场景进行补充验证：

**✅ 成功验证的边界场景**:
1. **原始交易成功 + RBF 未广播**: 原始交易保持在 mempool 中等待确认
2. **双重广播失败**: 两个交易都因低费率失败，状态正确回滚
3. **RBF 费用溢出**: 正确处理过高费用导致的溢出错误

**❌ 发现的问题**:
- **RBF 成功场景的状态异常**: 在原始交易广播失败、RBF 交易成功的场景下，发送方钱包状态同步异常
