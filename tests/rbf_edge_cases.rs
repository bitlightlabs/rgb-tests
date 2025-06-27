// RGB RBF (Replace-By-Fee) Edge Cases Test Suite
//
// This test suite covers various edge cases and boundary conditions for RBF functionality
// in RGB wallets, including broadcast failures, insufficient fees, and state management.
//
// Test scenarios covered:
// 1. Original transaction broadcasts successfully, RBF transaction fails to broadcast
// 2. Both original and RBF transactions fail to broadcast
// 3. Original transaction fails to broadcast, RBF fails due to insufficient fee
// 4. Original transaction fails to broadcast, RBF succeeds

pub mod utils;

use bpstd::Sats;
use bpwallet::psbt::TxParams;
use std::time::Duration;
use utils::chain::{get_height, stop_mining, tx_status};
use utils::helper::wallet::{get_wallet, AssetSchema};
use utils::{chain::initialize, DescriptorType, *};

#[test]
fn rbf_original_broadcast_rbf_unbroadcast() {
    initialize();

    // Create two wallet instances
    let mut wlt_1 = get_wallet(&DescriptorType::Wpkh);
    let mut wlt_2 = get_wallet(&DescriptorType::Wpkh);

    // Create and issue NIA asset
    let mut params = NIAIssueParams::new("RBFTestAsset", "RBF", "centiMilli", 600);
    let outpoint = wlt_1.get_utxo(None);
    params.add_allocation(outpoint, 600);
    let contract_id = wlt_1.issue_nia_with_params(params);
    wlt_1.send_contract("RBFTestAsset", &mut wlt_2);
    wlt_2.reload_runtime();

    let invoice = wlt_2.invoice(contract_id, 400, false, Some(0), None);

    // Stop mining to test RBF behavior
    stop_mining();
    let initial_height = get_height();

    // First transfer: broadcast successfully with lower fee
    let (consignment_1, _tx, payment) =
        wlt_1.transfer(invoice.clone(), None, Some(500), true, None);
    let first_txid = _tx.txid();
    dbg!(first_txid, tx_status(first_txid, wlt_1.instance));

    // Receiver accepts the transfer
    wlt_2.accept_transfer(&consignment_1, None).unwrap();

    // Verify block height hasn't changed (transaction not confirmed yet)
    let mid_height = get_height();
    assert_eq!(initial_height, mid_height);

    // Create RBF transaction with higher fee but don't broadcast it
    let _psbt = wlt_1.runtime.rbf(&payment, 1000_u64).unwrap();
    // Simulate RBF broadcast failure by not broadcasting

    // Sync wallets and verify balances
    wlt_1.sync();
    wlt_2.sync();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![200]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);

    // Print detailed wallet states after RBF scenario
    println!("=== RBF Original Broadcast, RBF Unbroadcast Test Results ===");
    println!("Scenario: Original transaction broadcasted successfully, RBF transaction created but not broadcasted");
    println!("Expected: Original transaction remains in mempool, wallet balances reflect pending transaction");
    println!();
    println!(
        "wlt_1 (sender) final balance: {:?}",
        wlt_1.get_allocation_sum(contract_id)
    );
    println!(
        "wlt_2 (receiver) final balance: {:?}",
        wlt_2.get_allocation_sum(contract_id)
    );
    println!();
    println!(
        "wlt_1 detailed allocations: {:?}",
        wlt_1.get_allocation_tuple(contract_id)
    );
    println!(
        "wlt_2 detailed allocations: {:?}",
        wlt_2.get_allocation_tuple(contract_id)
    );
    println!();
    println!(
        "Block height - Initial: {}, Final: {}",
        initial_height,
        get_height()
    );
    println!(
        "Transaction status - Original: {:?}",
        tx_status(first_txid, wlt_1.instance)
    );
    println!("=== Test Completed ===");
}

#[test]
fn rbf_both_original_and_rbf_unbroadcast() {
    initialize();

    let mut wlt_1 = get_wallet(&DescriptorType::Wpkh);
    let mut wlt_2 = get_wallet(&DescriptorType::Wpkh);

    let mut params = NIAIssueParams::new("RBFTestAsset", "RBF", "centiMilli", 600);
    let outpoint = wlt_1.get_utxo(None);
    params.add_allocation(outpoint, 600);
    let contract_id = wlt_1.issue_nia_with_params(params);
    wlt_1.send_contract("RBFTestAsset", &mut wlt_2);
    wlt_2.reload_runtime();

    let invoice = wlt_2.invoice(contract_id, 400, false, Some(0), None);

    // Stop mining to test RBF behavior
    stop_mining();
    // First transfer: broadcast with low fee (will fail due to insufficient fee)
    let params = TxParams::with(Sats(10));
    let (psbt, payment) = wlt_1
        .runtime
        .pay_invoice(
            &invoice,
            wlt_1.coinselect_strategy,
            params,
            Some(Sats(2000)),
        )
        .unwrap();
    let (consignment_1, _first_tx) = wlt_1.consign(
        contract_id,
        psbt,
        &payment.terminals,
        Duration::default(),
        false,
        None,
    );

    // Attempt to broadcast with low fee - should fail
    assert!(wlt_1.broadcast_tx(&_first_tx).is_err());

    // Receiver accepts the transfer but transaction is not on-chain
    wlt_2.accept_transfer(&consignment_1, None).unwrap();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);

    // After sync, state reverts since original transaction was not broadcasted
    wlt_1.sync();
    wlt_2.sync();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

    // Create RBF transaction but also don't broadcast it
    let mut psbt = wlt_1.runtime.rbf(&payment, 100u64).unwrap();
    let tx = wlt_1.sign_finalize_extract(&mut psbt);

    // RBF broadcast failure by fee
    assert!(wlt_1.broadcast_tx(&tx).is_err());

    // Verify wallet balances remain unchanged after both failures
    wlt_1.sync();
    wlt_2.sync();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

    // Print detailed wallet states after double failure scenario
    println!("=== RBF Both Original and RBF Unbroadcast Test Results ===");
    println!("Scenario: Both original transaction and RBF transaction failed to broadcast");
    println!("Expected: All wallet balances revert to initial state, no transactions on-chain");
    println!();
    println!(
        "wlt_1 (sender) final balance: {:?}",
        wlt_1.get_allocation_sum(contract_id)
    );
    println!(
        "wlt_2 (receiver) final balance: {:?}",
        wlt_2.get_allocation_sum(contract_id)
    );
    println!();
    println!(
        "wlt_1 detailed allocations: {:?}",
        wlt_1.get_allocation_tuple(contract_id)
    );
    println!(
        "wlt_2 detailed allocations: {:?}",
        wlt_2.get_allocation_tuple(contract_id)
    );
    println!();
    println!("Block height: {}", get_height());
    println!(
        "Result: Both transactions failed - wallet states correctly reverted to initial state"
    );
    println!("=== Test Completed ===");
}

#[test]
fn rbf_original_unbroadcast_rbf_insufficient_fee() {
    initialize();

    let mut wlt_1 = get_wallet(&DescriptorType::Wpkh);
    let mut wlt_2 = get_wallet(&DescriptorType::Wpkh);

    let mut params = NIAIssueParams::new("RBFTestAsset", "RBF", "centiMilli", 600);
    let outpoint = wlt_1.get_utxo(None);
    params.add_allocation(outpoint, 600);
    let contract_id = wlt_1.issue_nia_with_params(params);
    wlt_1.send_contract("RBFTestAsset", &mut wlt_2);
    wlt_2.reload_runtime();

    let invoice = wlt_2.invoice(contract_id, 400, false, Some(0), None);

    // Stop mining to test RBF behavior
    stop_mining();

    // First transfer: broadcast with low fee (will fail due to insufficient fee)
    let params = TxParams::with(Sats(100));
    let (psbt, payment) = wlt_1
        .runtime
        .pay_invoice(
            &invoice,
            wlt_1.coinselect_strategy,
            params,
            Some(Sats(2000)),
        )
        .unwrap();
    let (consignment_1, _first_tx) = wlt_1.consign(
        contract_id,
        psbt,
        &payment.terminals,
        Duration::default(),
        false,
        None,
    );

    // Attempt to broadcast with low fee - should fail
    assert!(wlt_1.broadcast_tx(&_first_tx).is_err());

    // Receiver accepts the transfer but transaction is not on-chain
    wlt_2.accept_transfer(&consignment_1, None).unwrap();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);

    // After sync, state reverts since original transaction was not broadcasted
    wlt_1.sync();
    wlt_2.sync();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

    // Attempt RBF with excessively high fee (should fail due to overflow)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wlt_1.runtime.rbf(&payment, 1_000_000_000_u64)
    }));
    assert!(result.is_err(), "Expected panic due to fee overflow");

    // Verify wallet balances remain unchanged after RBF failure
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
    wlt_1.sync();
    wlt_2.sync();
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

    // Print detailed wallet states after insufficient fee scenario
    println!("=== RBF Original Unbroadcast, RBF Insufficient Fee Test Results ===");
    println!("Scenario: Original transaction not broadcasted, RBF failed due to excessive fee (overflow)");
    println!(
        "Expected: RBF creation fails with overflow error, wallet balances remain at initial state"
    );
    println!();
    println!(
        "wlt_1 (sender) final balance: {:?}",
        wlt_1.get_allocation_sum(contract_id)
    );
    println!(
        "wlt_2 (receiver) final balance: {:?}",
        wlt_2.get_allocation_sum(contract_id)
    );
    println!();
    println!(
        "wlt_1 detailed allocations: {:?}",
        wlt_1.get_allocation_tuple(contract_id)
    );
    println!(
        "wlt_2 detailed allocations: {:?}",
        wlt_2.get_allocation_tuple(contract_id)
    );
    println!();
    println!("Block height: {}", get_height());
    println!("RBF Fee attempted: 1,000,000,000 sats (intentionally excessive)");
    println!("Result: RBF failed as expected - wallet states remain unchanged");
    println!("=== Test Completed ===");
}

#[test]
fn rbf_original_unbroadcast_rbf_success() {
    initialize();

    let mut wlt_1 = get_wallet(&DescriptorType::Wpkh);
    let mut wlt_2 = get_wallet(&DescriptorType::Wpkh);

    let mut params = NIAIssueParams::new("RBFTestAsset", "RBF", "centiMilli", 600);
    let outpoint = wlt_1.get_utxo(None);
    params.add_allocation(outpoint, 600);
    let contract_id = wlt_1.issue_nia_with_params(params);
    wlt_1.send_contract("RBFTestAsset", &mut wlt_2);
    wlt_2.reload_runtime();

    let invoice = wlt_2.invoice(contract_id, 400, false, Some(0), None);

    // Stop mining to test RBF behavior
    stop_mining();
    let initial_height = get_height();

    // First transfer: not broadcasted (simulating broadcast failure)
    let params = TxParams::with(Sats(100));
    let (psbt, payment) = wlt_1
        .runtime
        .pay_invoice(
            &invoice,
            wlt_1.coinselect_strategy,
            params,
            Some(Sats(2000)),
        )
        .unwrap();
    let (consignment_1, first_tx) = wlt_1.consign(
        contract_id,
        psbt,
        &payment.terminals,
        Duration::default(),
        false,
        None,
    );

    assert!(wlt_1.broadcast_tx(&first_tx).is_err());
    let first_txid = first_tx.txid();

    // Receiver accepts the transfer but transaction is not on-chain
    wlt_2.accept_transfer(&consignment_1, None).unwrap();

    // Check state after first failed broadcast
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);
    wlt_1.sync();
    wlt_2.sync();
    // After sync, state reverts since first transaction was not broadcasted
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![600]);
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![]);

    // RBF attempt: create replacement transaction with higher fee and broadcast successfully
    let (consignment_2, tx) = wlt_1.transfer_rbf(contract_id, payment, 1000, None);
    let second_txid = tx.txid();

    // Broadcast and confirm the RBF transaction
    wlt_1.mine_tx(&tx.txid(), true);

    // Receiver accepts the successful RBF transfer
    wlt_2.accept_transfer(&consignment_2, None).unwrap();

    // Sync both wallets
    wlt_1.sync();
    wlt_2.sync();

    dbg!(wlt_1.runtime.state_all(contract_id).owned);
    dbg!(wlt_1.runtime.state_own(contract_id).owned);
    // Verify block height changed (RBF transaction was confirmed)
    let final_height = get_height();
    assert!(final_height > initial_height);

    // Print detailed wallet states after successful RBF scenario
    println!("=== RBF Original Unbroadcast, RBF Success Test Results ===");
    println!("Scenario: Original transaction not broadcasted, RBF transaction successfully broadcasted and confirmed");
    println!("Expected: RBF transaction replaces original, final transfer completes successfully");
    println!();
    println!("Transaction Details:");
    println!(
        "  Original TX ID: {} (status: {:?})",
        first_txid,
        tx_status(first_txid, wlt_1.instance)
    );
    println!(
        "  RBF TX ID: {} (status: {:?})",
        second_txid,
        tx_status(second_txid, wlt_1.instance)
    );
    println!();
    println!(
        "wlt_1 (sender) final balance: {:?}",
        wlt_1.get_allocation_sum(contract_id)
    );
    println!(
        "wlt_2 (receiver) final balance: {:?}",
        wlt_2.get_allocation_sum(contract_id)
    );
    println!();
    println!(
        "wlt_1 detailed allocations: {:?}",
        wlt_1.get_allocation_tuple(contract_id)
    );
    println!(
        "wlt_2 detailed allocations: {:?}",
        wlt_2.get_allocation_tuple(contract_id)
    );
    println!();
    println!(
        "Block height - Initial: {}, Final: {} (increased by {})",
        initial_height,
        final_height,
        final_height - initial_height
    );
    println!("Transfer amount: 400 tokens");
    println!("RBF fee: 1000 sats");
    wlt_2.check_allocations(contract_id, AssetSchema::RGB20, vec![400]);
    // Verify final asset allocations - RBF transaction succeeded
    wlt_1.check_allocations(contract_id, AssetSchema::RGB20, vec![200]);
    println!("Result: RBF transaction successfully replaced original and completed transfer");
    println!("=== Test Completed ===");
}
