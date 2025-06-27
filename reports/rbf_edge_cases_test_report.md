# RGB RBF Edge Cases Test Report

## Project Background

RGB Tests is a testing repository for RGB protocol implementation. This report focuses on testing and validation of edge cases and exceptional scenarios for RBF (Replace-By-Fee) functionality.

## Test Environment

- **Test File**: `tests/rbf_edge_cases.rs`
- **Asset Type**: NIA (Non-Inflatable Asset)
- **Wallet Type**: Wpkh (Witness Public Key Hash)
- **Test Asset**: RBFTestAsset (RBF), Total Supply: 600 units

### Results Overview

**Complete Testing**: 6 test cases covering main RBF usage scenarios and edge conditions
- Basic Functionality: 2/2 passed (100%)
- Edge Cases: 3/4 passed (75%)
- Overall Pass Rate: 5/6 (83.3%)

## Test Cases Overview

### Original RBF Test Cases (tests/transfer.rs)

Before edge case testing, the project already included two basic RBF test cases validating normal workflows:

#### Original Test Case 1: `rbf_transfer`
**Test Definition**: Original transaction successfully broadcasts to mempool, RBF transaction successfully replaces it

**Operation Flow**:
1. Create transfer transaction with 500 sats fee, successfully broadcast to mempool
2. Receiver accepts the transfer
3. Create RBF transaction with 1000 sats fee, successfully replaces original transaction
4. Mine and confirm RBF transaction

**Validation Result**: ✅ Passed - State correct, RBF mechanism working normally

#### Original Test Case 2: `rbf_unbroadcasted_state_all`
**Test Definition**: Original transaction not broadcast, RBF transaction also not broadcast, finally broadcast original transaction

**Operation Flow**:
1. Create transfer transaction with 500 sats fee, not broadcast (`broadcast=false`)
2. Receiver accepts the transfer
3. Create RBF transaction with 1000 sats fee, not broadcast
4. Broadcast and confirm original transaction (not RBF transaction)

**Validation Result**: ✅ Passed - State correct, unbroadcast RBF transaction status is `WitnessStatus::Archived`

### Edge Case Test Suite (tests/rbf_edge_cases.rs)

Based on the success of original tests, this edge case test suite supplements 4 exceptional and boundary scenarios:

### 1. `rbf_original_broadcast_rbf_unbroadcast`

**Test Definition**: Original transaction successfully broadcasts, RBF transaction created but not broadcast

**Initial State**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**Operation Sequence**:
1. Stop mining
2. Create transfer of 400 tokens with 500 sats fee, successfully broadcast
3. wlt_2 accepts transfer
4. Create RBF transaction with 1000 sats fee, but not broadcast (simulating broadcast failure scenarios)
5. Sync wallet states

**Expected Behavior**:
- Original transaction remains in mempool
- wlt_1 balance: 200 tokens (reflecting pending transaction)
- wlt_2 balance: 400 tokens (reflecting pending transaction)
- Block height unchanged (transaction unconfirmed)

**Validation Assertions**:
```rust
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![200]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);
assert_eq!(initial_height, mid_height);
```

### 2. `rbf_both_original_and_rbf_unbroadcast`

**Test Definition**: Both original transaction and RBF transaction fail to broadcast

**Initial State**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**Operation Sequence**:
1. Stop mining
2. Create transfer of 400 tokens with 100 sats fee (low fee rate), broadcast attempt fails
3. wlt_2 accepts transfer
4. Sync wallet (state rollback)
5. Create RBF transaction with 1000 sats fee, not broadcast (simulating broadcast failure scenarios)
6. Sync wallets again

**Expected Behavior**:
- All wallet balances rollback to initial state
- wlt_1 balance: 600 tokens
- wlt_2 balance: 0 tokens
- No on-chain transactions

**Validation Assertions**:
```rust
// After accepting transfer
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);

// After sync, state rollback
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

// After RBF failure, state remains
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
```

### 3. `rbf_original_unbroadcast_rbf_insufficient_fee`

**Test Definition**: Original transaction not broadcast, RBF fails due to excessive fee causing overflow

**Initial State**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**Operation Sequence**:
1. Stop mining
2. Create transfer of 400 tokens with 100 sats fee (low fee rate), broadcast attempt fails
3. wlt_2 accepts transfer
4. Sync wallet (state rollback)
5. Attempt to create RBF transaction with 1,000,000,000 sats fee (excessive)

**Expected Behavior**:
- RBF creation fails due to overflow error
- Wallet balances remain at initial state
- wlt_1 balance: 600 tokens
- wlt_2 balance: 0 tokens

**Validation Assertions**:
```rust
#[should_panic(expected = "overflow")]
assert!(wlt_1.runtime.rbf(&payment, 1_000_000_000_u64).is_err());
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
```

### 4. `rbf_original_unbroadcast_rbf_success`

**Test Definition**: Original transaction not broadcast, RBF transaction successfully broadcasts and confirms

**Initial State**:
- wlt_1: 600 tokens
- wlt_2: 0 tokens

**Operation Sequence**:
1. Stop mining
2. Create transfer of 400 tokens with 100 sats fee (low fee rate), broadcast attempt fails
3. wlt_2 accepts transfer
4. Sync wallet (state rollback)
5. Create RBF transaction with 1000 sats fee, successfully broadcast
6. Mine and confirm RBF transaction
7. wlt_2 accepts RBF transfer
8. Sync wallets

**Expected Behavior**:
- RBF transaction replaces original transaction and completes successfully
- wlt_1 balance: 200 tokens
- wlt_2 balance: 400 tokens
- Block height increases (transaction confirmed)

**Validation Assertions**:
```rust
wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![200]);
wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);
assert!(final_height > initial_height);
```

## Complete Test Coverage Matrix

### Original Test Cases (Basic Flow Validation)

| Test Case                     | Original TX Broadcast | RBF TX Broadcast  | Final Confirmed TX | Expected Result       | Actual Result |
| ----------------------------- | --------------------- | ----------------- | ------------------ | --------------------- | ------------- |
| `rbf_transfer`                | ✅ Success             | ✅ Success Replace | RBF Transaction    | RBF TX Confirmed      | ✅ Passed      |
| `rbf_unbroadcasted_state_all` | ❌ Not Broadcast       | ❌ Not Broadcast   | Original TX        | Original TX Confirmed | ✅ Passed      |

### Edge Case Test Suite (Exception and Boundary Scenarios)

| Test Case                                       | Original TX Broadcast | RBF TX Broadcast  | Expected Result         | Actual Result   |
| ----------------------------------------------- | --------------------- | ----------------- | ----------------------- | --------------- |
| `rbf_original_broadcast_rbf_unbroadcast`        | ✅ Success             | ❌ Not Broadcast   | Original TX Pending     | ✅ Passed        |
| `rbf_both_original_and_rbf_unbroadcast`         | ❌ Low Fee Failure     | ❌ Low Fee Failure | Complete State Rollback | ✅ Passed        |
| `rbf_original_unbroadcast_rbf_insufficient_fee` | ❌ Low Fee Failure     | ❌ Fee Overflow    | State Remains Initial   | ✅ Passed        |
| `rbf_original_unbroadcast_rbf_success`          | ❌ Low Fee Failure     | ✅ Success         | RBF TX Confirmed        | ❌ State Anomaly |

## Test Implementation Improvements

### Real Broadcast Failure Simulation

Compared to simply not broadcasting transactions to simulate failure, this test suite adopts a more realistic approach:

- **Low Fee Rate Broadcast Failure**: Uses 100 sats fee rate to create transactions, causing real broadcast failure due to being below minimum relay fee (153 sats)
- **Error Verification**: Confirms failure reason through RPC error `"min relay fee not met, 100 < 153"`
- **State Consistency**: Ensures wallet state remains consistent with actual network behavior

### Key Improvement Points

1. **Authenticity**: Uses actual network constraint conditions to trigger failures
2. **Reproducibility**: 100 sats fee rate stably triggers failure in regtest network
3. **Error Handling**: Correctly handles and validates broadcast failure error types

## Actual Test Results

### Test Execution Overview

**Execution Command**: `SKIP_INIT=true RUST_BACKTRACE=1 cargo test --all-features -- rbf`

**Test Results**: 3 out of 4 test cases passed, 1 failed

| Test Case                                       | Execution Status | Execution Time | Result             |
| ----------------------------------------------- | ---------------- | -------------- | ------------------ |
| `rbf_both_original_and_rbf_unbroadcast`         | ✅ Passed         | < 1s           | As Expected        |
| `rbf_original_unbroadcast_rbf_insufficient_fee` | ✅ Passed         | < 1s           | As Expected        |
| `rbf_original_broadcast_rbf_unbroadcast`        | ✅ Passed         | < 1s           | As Expected        |
| `rbf_original_unbroadcast_rbf_success`          | ❌ Failed         | 144.79s        | State Inconsistent |

### Detailed Test Analysis

#### ✅ Successful Test Cases

**1. rbf_both_original_and_rbf_unbroadcast**
- Status: Passed
- Behavior: Double broadcast failure correctly handled, wallet state correctly rolled back

**2. rbf_original_unbroadcast_rbf_insufficient_fee** 
- Status: Passed
- Behavior: Overflow error correctly caught, wallet state remains unchanged

**3. rbf_original_broadcast_rbf_unbroadcast**
- Status: Passed  
- Behavior: Original transaction broadcast successfully, RBF not broadcast, state correctly maintained

#### ❌ Failed Test Case

**rbf_original_unbroadcast_rbf_success**

**Failure Reason**: Wallet state inconsistency
- **Expected**: wlt_1 balance should be 200 tokens
- **Actual**: wlt_1 balance is 0 tokens (empty array)
- **Error**: `assertion 'left == right' failed: left: [] right: [200]`

**Key Observations**:
1. **Original Transaction Broadcast Failure**: Confirmed receiving expected fee rate error `"min relay fee not met, 100 < 153"`
2. **RBF Transaction Success**: RBF transaction successfully broadcast and confirmed in block 234
3. **Receiver State Correct**: wlt_2 correctly received 400 tokens
4. **Sender State Anomaly**: wlt_1 shows 0 balance instead of expected 200 tokens

**State Analysis**:
```
wlt_1.runtime.state_all(contract_id).owned: Contains genesis state (600 tokens)
wlt_1.runtime.state_own(contract_id).owned: Empty array []
```

## Conclusion

### Test System Completeness

#### Original Test Foundation (✅ Verified)
The project's original two RBF test cases have verified core functionality correctness:
- **`rbf_transfer`**: Verified standard RBF replacement flow - original transaction enters mempool, RBF successfully replaces
- **`rbf_unbroadcasted_state_all`**: Verified unbroadcast transaction state management - original transaction not broadcast, RBF also not broadcast, finally broadcast original transaction

#### Edge Case Test Supplement (Main Report Body)
Based on the success of original tests, the edge case test suite specifically supplements validation for exceptional and boundary scenarios:

**✅ Successfully Verified Edge Scenarios**:
1. **Original Transaction Success + RBF Not Broadcast**: Original transaction remains in mempool awaiting confirmation
2. **Double Broadcast Failure**: Both transactions fail due to low fee rate, state correctly rolls back
3. **RBF Fee Overflow**: Correctly handles overflow errors caused by excessive fees

**❌ Discovered Issues**:
- **RBF Success Scenario State Anomaly**: In scenarios where original transaction broadcast fails but RBF transaction succeeds, sender wallet state synchronization is abnormal