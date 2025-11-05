// RGB Transfer Tests for Lightning RGB Wallet
//
// This file contains transfer tests adapted from rgb-tests/tests/transfer.rs
// Simplified for NIA (Non-Inflatable Asset) / RGB20 only, regtest network only

use bpstd::Network;
use ln_rgb_tests::utils::{asset_params::NIAIssueParams, chain, wallet::*, *};
use serial_test::{serial, parallel};

/// Test basic NIA transfer using Witness type
///
/// This test performs a simple RGB20 asset transfer:
/// 1. Create two wallets
/// 2. Fund and issue asset on wallet 1
/// 3. Share contract info with wallet 2
/// 4. Transfer asset from wallet 1 to wallet 2 (Witness type)
/// 5. Verify final allocations
#[tokio::test]
#[serial(parallel)]
async fn test_basic_nia_transfer_witness() {
    println!("\n=== Test: Basic NIA Transfer (Witness) ===\n");

    // Initialize test environment
    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create two test wallets
    println!("Creating wallets...");
    let wlt_1 = create_test_wallet("transfer_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("transfer_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");

    println!("✓ Wallets created");

    // Fund wallet 1 with multiple UTXOs
    // - One for asset issuance
    // - One for transaction fees during transfer
    println!("\nFunding wallet 1...");
    let utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO for issuance");
    println!("✓ Wallet 1 funded with issuance UTXO: {}", utxo);

    // Get another UTXO for transaction fees
    let _fee_utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO for fees");
    println!("✓ Wallet 1 funded with fee UTXO");

    // Issue NIA asset on wallet 1
    let issued_supply = 999u64;
    println!("\nIssuing NIA asset...");
    println!("  Name: TestAsset1");
    println!("  Ticker: TEST1");
    println!("  Supply: {}", issued_supply);

    let mut params = NIAIssueParams::new("TestAsset1", "TEST1", "centiMilli", issued_supply);
    params.add_allocation(utxo, issued_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);

    // Share contract info with wallet 2
    println!("\nSharing contract with wallet 2...");
    share_contract(&wlt_1, &wlt_2, contract_id, "TestAsset1")
        .await
        .expect("Failed to share contract");
    println!("✓ Contract shared");

    // Verify initial allocations
    println!("\nVerifying initial allocations...");
    check_allocations(&wlt_1, contract_id, vec![issued_supply])
        .await
        .expect("Initial allocation check failed for wallet 1");

    // Wallet 1 transfers 99 tokens to wallet 2 (Witness type)
    let transfer_amount = 99u64;
    println!(
        "\nTransferring {} tokens from wallet 1 to wallet 2...",
        transfer_amount
    );

    // Create invoice (Witness type)
    let invoice = wlt_2
        .create_invoice(
            contract_id,
            transfer_amount,
            true, // use_witness_utxo = true (Witness transfer)
            None, // nonce
            None, // blinding_utxo not needed for Witness
        )
        .await;
    println!("✓ Invoice created");

    // Execute transfer
    let (consignment_key, tx, _payment) = wlt_1
        .transfer(invoice, Some(2000), None, true)
        .await
        .expect("Transfer failed");
    println!("✓ Transfer executed, tx: {}", tx.txid());

    // Mine block to confirm transaction
    chain::mine(false);
    println!("✓ Transaction confirmed");

    // Sync wallets
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ Wallets synced");

    // Wallet 2 accepts the transfer
    println!("\nWallet 2 accepting transfer...");
    wlt_2
        .accept_transfer(consignment_key)
        .await
        .expect("Failed to accept transfer");
    println!("✓ Transfer accepted");

    // Sync wallet 2 again after acceptance
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");

    // Verify final allocations
    println!("\nVerifying final allocations...");
    let expected_wlt_1 = issued_supply - transfer_amount;
    let expected_wlt_2 = transfer_amount;

    check_allocations(&wlt_1, contract_id, vec![expected_wlt_1])
        .await
        .expect("Final allocation check failed for wallet 1");

    check_allocations(&wlt_2, contract_id, vec![expected_wlt_2])
        .await
        .expect("Final allocation check failed for wallet 2");

    println!("\n=== Test Passed ===");
    println!("  Wallet 1 final balance: {}", expected_wlt_1);
    println!("  Wallet 2 final balance: {}", expected_wlt_2);

    // Cleanup
    cleanup_test_wallet("transfer_wlt_1").await.ok();
    cleanup_test_wallet("transfer_wlt_2").await.ok();
}

/// Test basic NIA transfer using Blinded type
///
/// Similar to witness transfer, but using blinded UTXO
#[tokio::test]
#[serial(parallel)]
async fn test_basic_nia_transfer_blinded() {
    println!("\n=== Test: Basic NIA Transfer (Blinded) ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallets
    println!("Creating wallets...");
    let wlt_1 = create_test_wallet("blinded_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("blinded_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");
    println!("✓ Wallets created");

    // Fund wallet 1 for issuance
    println!("\nFunding wallet 1...");
    let utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO");
    println!("✓ Wallet 1 funded");

    // Issue asset
    let issued_supply = 999u64;
    println!("\nIssuing NIA asset (supply: {})...", issued_supply);
    let mut params = NIAIssueParams::new("BlindedAsset", "BLIND", "centiMilli", issued_supply);
    params.add_allocation(utxo, issued_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);

    // Share contract
    share_contract(&wlt_1, &wlt_2, contract_id, "BlindedAsset")
        .await
        .expect("Failed to share contract");

    // Fund wallet 2 to get blinding UTXO
    println!("\nPreparing blinding UTXO for wallet 2...");
    let blinding_utxo = get_utxo(&wlt_2, Some(5_000), INSTANCE_1)
        .await
        .expect("Failed to get blinding UTXO");
    println!("✓ Blinding UTXO prepared: {}", blinding_utxo);

    // Transfer with Blinded type
    let transfer_amount = 99u64;
    println!("\nTransferring {} tokens (Blinded)...", transfer_amount);

    // Create blinded invoice
    let invoice = wlt_2
        .create_invoice(
            contract_id,
            transfer_amount,
            false,               // use_witness_utxo = false (Blinded transfer)
            None,                // nonce
            Some(blinding_utxo), // blinding_utxo required for Blinded
        )
        .await;
    println!("✓ Blinded invoice created");

    // Execute transfer
    let (consignment_key, tx, _payment) = wlt_1
        .transfer(invoice, Some(2000), None, true)
        .await
        .expect("Transfer failed");
    println!("✓ Transfer executed, tx: {}", tx.txid());

    // Confirm and sync
    chain::mine(false);
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ Transaction confirmed and wallets synced");

    // Accept transfer
    println!("\nWallet 2 accepting blinded transfer...");
    wlt_2
        .accept_transfer(consignment_key)
        .await
        .expect("Failed to accept transfer");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ Transfer accepted");

    // Verify final allocations
    println!("\nVerifying final allocations...");
    let expected_wlt_1 = issued_supply - transfer_amount;
    let expected_wlt_2 = transfer_amount;

    check_allocations(&wlt_1, contract_id, vec![expected_wlt_1])
        .await
        .expect("Final allocation check failed for wallet 1");

    check_allocations(&wlt_2, contract_id, vec![expected_wlt_2])
        .await
        .expect("Final allocation check failed for wallet 2");

    println!("\n=== Test Passed ===");
    println!("  Wallet 1 final balance: {}", expected_wlt_1);
    println!("  Wallet 2 final balance: {}", expected_wlt_2);

    // Cleanup
    cleanup_test_wallet("blinded_wlt_1").await.ok();
    cleanup_test_wallet("blinded_wlt_2").await.ok();
}

/// Test multiple sequential transfers
///
/// Wallet 1 sends multiple amounts to wallet 2 in sequence
#[tokio::test]
#[serial(parallel)]
async fn test_multiple_transfers() {
    println!("\n=== Test: Multiple Sequential Transfers ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallets
    let wlt_1 = create_test_wallet("multi_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("multi_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");

    // Fund and issue
    let utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO");

    let issued_supply = 999u64;
    let mut params = NIAIssueParams::new("MultiAsset", "MULTI", "centiMilli", issued_supply);
    params.add_allocation(utxo, issued_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");

    share_contract(&wlt_1, &wlt_2, contract_id, "MultiAsset")
        .await
        .expect("Failed to share contract");

    println!("✓ Initial setup completed");

    // Perform three transfers
    let transfers = vec![99u64, 33u64, 22u64];
    let mut wlt_1_balance = issued_supply;
    let mut wlt_2_balance = 0u64;

    for (i, &amount) in transfers.iter().enumerate() {
        println!("\n--- Transfer #{} ({} tokens) ---", i + 1, amount);

        // Create invoice
        let invoice = wlt_2
            .create_invoice(contract_id, amount, true, None, None)
            .await;

        // Transfer
        let (consignment_key, tx, _payment) = wlt_1
            .transfer(invoice, Some(2000), None, true)
            .await
            .expect(&format!("Transfer #{} failed", i + 1));

        println!("  Transfer tx: {}", tx.txid());

        // Confirm
        chain::mine(false);
        wlt_1.sync_runtime().await.unwrap();
        wlt_2.sync_runtime().await.unwrap();

        // Accept
        wlt_2.accept_transfer(consignment_key).await.unwrap();
        wlt_2.sync_runtime().await.unwrap();

        // Update expected balances
        wlt_1_balance -= amount;
        wlt_2_balance += amount;

        println!("  ✓ Transfer #{} completed", i + 1);
    }

    // Verify final allocations
    println!(
        "\nVerifying final state after {} transfers...",
        transfers.len()
    );

    // Wallet 1 should have: 999 - 99 - 33 - 22 = 845
    check_allocations(&wlt_1, contract_id, vec![wlt_1_balance])
        .await
        .expect("Final allocation check failed for wallet 1");

    // Wallet 2 should have: [99, 33, 22] (or sorted: [22, 33, 99])
    check_allocations(&wlt_2, contract_id, vec![99, 33, 22])
        .await
        .expect("Final allocation check failed for wallet 2");

    println!("\n=== Test Passed ===");
    println!("  Wallet 1 final balance: {}", wlt_1_balance);
    println!(
        "  Wallet 2 final balance: {} (in {} allocations)",
        wlt_2_balance,
        transfers.len()
    );

    // Cleanup
    cleanup_test_wallet("multi_wlt_1").await.ok();
    cleanup_test_wallet("multi_wlt_2").await.ok();
}

/// Test wallet reload after issuing asset
///
/// This test verifies that wallet state can be properly persisted and reloaded:
/// 1. Create wallet and issue asset
/// 2. Shutdown wallet completely
/// 3. Reload wallet from disk
/// 4. Verify asset state is preserved
#[tokio::test]
#[serial(parallel)]
async fn test_wallet_reload_after_issue() {
    println!("\n=== Test: Wallet Reload After Issue ===\n");

    // Initialize test environment
    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);
    let wallet_name = "reload_test_wallet";

    // Clean up any existing wallet data
    let wallet_dir = get_test_wallet_dir_fixed(wallet_name);
    if wallet_dir.exists() {
        println!("Cleaning up old wallet data...");
        std::fs::remove_dir_all(&wallet_dir).expect("Failed to cleanup wallet dir");
    }

    // Phase 1: Create wallet and issue asset
    println!("\n--- Phase 1: Create wallet and issue asset ---");
    let contract_id = {
        let wlt = reuse_test_wallet(wallet_name, network, indexer_url.clone())
            .await
            .expect("Failed to create wallet");

        println!("✓ Wallet created");

        // Fund wallet
        let utxo = get_utxo(&wlt, Some(10_000), INSTANCE_1)
            .await
            .expect("Failed to get UTXO");
        println!("✓ Wallet funded with UTXO: {}", utxo);

        // Issue asset
        let mut params = NIAIssueParams::new("ReloadTest", "RLT", "centi", 1_000_000);
        params.add_allocation(utxo, 1_000_000);

        let contract_id = wlt
            .issue(params.into_create_params())
            .await
            .expect("Failed to issue asset");
        println!("✓ Asset issued: {}", contract_id);

        // Verify allocations before shutdown
        let allocations = wlt
            .get_allocations(contract_id)
            .await
            .expect("Failed to get allocations");
        println!("✓ Allocations before shutdown: {} UTXOs", allocations.len());
        assert_eq!(allocations.len(), 1, "Should have 1 allocation");
        assert_eq!(
            allocations[0].1, 1_000_000,
            "Allocation amount should be 1,000,000"
        );

        // Commit delta to base before shutdown
        println!("\nCommitting delta stockpile to base...");
        // wlt.commit()
        //     .await
        //     .expect("Failed to commit stockpile");
        // println!("✓ Delta committed to base");

        // Shutdown wallet
        println!("\nShutting down wallet...");
        wlt.shutdown().await;
        drop(wlt);
        println!("✓ Wallet shut down");

        contract_id
    };

    // todo!();

    // Give some time for cleanup
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // Phase 2: Reload wallet and verify state
    println!("\n--- Phase 2: Reload wallet and verify state ---");
    {
        let wlt = reuse_test_wallet(wallet_name, network, indexer_url)
            .await
            .expect("Failed to reload wallet");

        println!("✓ Wallet reloaded");

        let c = wlt.list_contracts().await.unwrap();
        dbg!(c);

        // Verify contract still exists
        let allocations = wlt
            .get_allocations(contract_id)
            .await
            .expect("Failed to get allocations after reload");

        println!("✓ Allocations after reload: {} UTXOs", allocations.len());
        assert_eq!(allocations.len(), 1, "Should still have 1 allocation");
        assert_eq!(
            allocations[0].1, 1_000_000,
            "Allocation amount should still be 1,000,000"
        );

        println!("\n=== Test Passed ===");
        println!("  Contract ID: {}", contract_id);
        println!("  Allocation preserved: {} sats", allocations[0].1);

        // Cleanup
        wlt.shutdown().await;
    }

    // Cleanup wallet data
    if wallet_dir.exists() {
        std::fs::remove_dir_all(&wallet_dir).ok();
    }
}

/// Test sending asset to oneself
///
/// This test verifies self-transfer capability:
/// 1. Issue asset with full supply on one UTXO
/// 2. Transfer part of the asset to self
/// 3. Verify change handling creates two allocations
///
/// This is important for Lightning Network where change management is critical
#[tokio::test]
#[serial(parallel)]
async fn test_send_to_oneself() {
    println!("\n=== Test: Send to Oneself ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallet
    println!("Creating wallet...");
    let wlt = create_test_wallet("self_transfer_wlt", network, indexer_url)
        .await
        .expect("Failed to create wallet");
    println!("✓ Wallet created");

    // Fund wallet for issuance
    println!("\nFunding wallet...");
    let utxo = get_utxo(&wlt, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO");
    println!("✓ Wallet funded with UTXO: {}", utxo);

    // Issue asset with supply of 600
    let issued_supply = 600u64;
    println!("\nIssuing NIA asset (supply: {})...", issued_supply);
    let mut params = NIAIssueParams::new("SelfTestAsset", "SELF", "centiMilli", issued_supply);
    params.add_allocation(utxo, issued_supply);

    let contract_id = wlt
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);

    // Verify initial allocation - should be one UTXO with full supply
    println!("\nVerifying initial allocation...");
    check_allocations(&wlt, contract_id, vec![issued_supply])
        .await
        .expect("Initial allocation check failed");
    println!("✓ Initial: 1 allocation with {} tokens", issued_supply);

    // Transfer 200 tokens to self
    let transfer_amount = 200u64;
    println!(
        "\nTransferring {} tokens to self...",
        transfer_amount
    );

    // Create invoice for self (Witness type)
    let invoice = wlt
        .create_invoice(
            contract_id,
            transfer_amount,
            true, // use_witness_utxo = true
            Some(0), // nonce
            None, // no blinding_utxo
        )
        .await;
    println!("✓ Self-invoice created");

    // Execute transfer
    let (consignment_key, tx, _payment) = wlt
        .transfer(invoice.clone(), Some(2000), None, true)
        .await
        .expect("Self-transfer failed");
    println!("✓ Transfer executed, tx: {}", tx.txid());

    // Mine block to confirm
    chain::mine(false);
    println!("✓ Transaction confirmed");

    // Sync wallet
    wlt.sync_runtime().await.expect("Failed to sync wallet");
    println!("✓ Wallet synced");

    // Accept own transfer
    println!("\nAccepting self-transfer...");
    wlt.accept_transfer(consignment_key)
        .await
        .expect("Failed to accept self-transfer");
    wlt.sync_runtime().await.expect("Failed to sync after accept");
    println!("✓ Self-transfer accepted");

    // Verify final allocations - should have TWO allocations now:
    // - One with transfer_amount (200)
    // - One with change (issued_supply - transfer_amount = 400)
    println!("\nVerifying final allocations...");
    let expected_change = issued_supply - transfer_amount;
    check_allocations(&wlt, contract_id, vec![transfer_amount, expected_change])
        .await
        .expect("Final allocation check failed");

    println!("\n=== Test Passed ===");
    println!("  Initial: 1 UTXO with {} tokens", issued_supply);
    println!("  Transferred to self: {} tokens", transfer_amount);
    println!("  Final: 2 UTXOs with {} and {} tokens", transfer_amount, expected_change);
    println!("  Change handling verified ✓");

    // Cleanup
    cleanup_test_wallet("self_transfer_wlt").await.ok();
}

/// Test accepting transfer without confirmation (0-conf)
///
/// **Important Note: Why original Witness 0-conf tests pass in rgb-tests:**
///
/// In the original rgb-tests implementation:
/// 1. accept_transfer() method calls self.sync() at the beginning
/// 2. sync() fetches **unconfirmed transactions** from mempool via indexer (Esplora/Electrum)
/// 3. Witness UTXOs in these unconfirmed transactions are **registered to wallet**
/// 4. Later when consume_from_file() parses consignment, it can find corresponding RGB state
/// 5. check_allocations() → state_own() → wallet.has_utxo() returns true
///
/// Key code locations:
/// - rgb-tests/tests/utils/helper/wallet.rs:810 - accept_transfer() calls sync()
/// - rgb-std/src/popls/bp.rs:434 - state_own() checks wallet.has_utxo()
///
/// **Difference in Current Implementation:**
/// We use **Blinded transfer** instead of Witness, because:
/// - Blinded: Uses receiver's **pre-existing UTXO** (blinding_utxo)
/// - Witness: Requires syncing from mempool to get newly created UTXO
///
/// **Lightning Network Requirements:**
/// In Lightning Network, we need a way to **check witness RGB state without relying on sync**,
/// because commitment TX may not be broadcasted to mempool (exists only off-chain).
/// We will discuss solutions to this problem later.
///
/// This test verifies zero-confirmation transfer acceptance:
/// 1. Create transfer using Blinded type but don't mine
/// 2. Receiver accepts the unconfirmed transfer
/// 3. Verify receiver sees allocation immediately (0-conf)
/// 4. Mine block to confirm
/// 5. Verify sender sees change allocation
#[tokio::test]
#[serial(parallel)]
async fn test_accept_0conf() {
    println!("\n=== Test: Accept 0-Conf Transfer ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create two wallets
    println!("Creating wallets...");
    let wlt_1 = create_test_wallet("0conf_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("0conf_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");
    println!("✓ Wallets created");

    // Fund wallet 1 and issue asset
    println!("\nFunding and issuing asset...");
    let utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO");

    let issued_supply = 600u64;
    let mut params = NIAIssueParams::new("ZeroConfAsset", "0CONF", "centiMilli", issued_supply);
    params.add_allocation(utxo, issued_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);

    // Share contract with wallet 2
    share_contract(&wlt_1, &wlt_2, contract_id, "ZeroConfAsset")
        .await
        .expect("Failed to share contract");
    println!("✓ Contract shared");

    // Fund wallet 2 to get blinding UTXO (for 0-conf to work reliably)
    println!("\nPreparing blinding UTXO for wallet 2...");
    let blinding_utxo = get_utxo(&wlt_2, Some(5_000), INSTANCE_1)
        .await
        .expect("Failed to get blinding UTXO");
    println!("✓ Blinding UTXO prepared: {}", blinding_utxo);

    // Create transfer but DON'T mine yet
    let transfer_amount = 200u64;
    println!(
        "\nCreating transfer of {} tokens (no mining yet)...",
        transfer_amount
    );

    // Use Blinded invoice instead of Witness for 0-conf
    let invoice = wlt_2
        .create_invoice(
            contract_id,
            transfer_amount,
            false,               // use_witness_utxo = false (Blinded)
            Some(0),             // nonce
            Some(blinding_utxo), // blinding_utxo
        )
        .await;

    let (consignment_key, tx, _payment) = wlt_1
        .transfer(invoice, Some(2000), None, true)
        .await
        .expect("Transfer failed");

    let txid = tx.txid();
    println!("✓ Transfer created, tx: {} (unconfirmed)", txid);

    // Receiver accepts transfer WITHOUT mining (0-conf)
    println!("\nWallet 2 accepting unconfirmed transfer (0-conf)...");
    wlt_2
        .accept_transfer(consignment_key)
        .await
        .expect("Failed to accept 0-conf transfer");
    println!("✓ 0-conf transfer accepted");

    // FIXME:
    // TODO: Need to add a witness-output type for receiving assets, as this is how Lightning Network works

    // Wallet 2 should see the allocation immediately, even without mining
    println!("\nVerifying wallet 2 sees allocation (before mining)...");
    check_allocations(&wlt_2, contract_id, vec![transfer_amount])
        .await
        .expect("Wallet 2 should see allocation in 0-conf");
    println!("✓ Wallet 2 sees {} tokens (0-conf)", transfer_amount);

    // Now mine the transaction to confirm
    println!("\nMining transaction...");
    chain::mine(false);
    println!("✓ Transaction confirmed");

    // Sync wallet 1
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");

    // Wallet 1 should now see the change allocation
    let expected_change = issued_supply - transfer_amount;
    println!("\nVerifying wallet 1 sees change (after mining)...");
    check_allocations(&wlt_1, contract_id, vec![expected_change])
        .await
        .expect("Wallet 1 should see change after mining");
    println!("✓ Wallet 1 sees {} tokens (change)", expected_change);

    // Sync wallet 2 and verify again
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    check_allocations(&wlt_2, contract_id, vec![transfer_amount])
        .await
        .expect("Wallet 2 allocation check failed after mining");

    println!("\n=== Test Passed ===");
    println!("  Transfer amount: {}", transfer_amount);
    println!("  0-conf acceptance: ✓");
    println!("  Wallet 2 (receiver): {} tokens", transfer_amount);
    println!("  Wallet 1 (sender change): {} tokens", expected_change);

    // Cleanup
    cleanup_test_wallet("0conf_wlt_1").await.ok();
    cleanup_test_wallet("0conf_wlt_2").await.ok();
}

/// Test invoice reuse - same invoice can be paid multiple times
///
/// This test verifies that RGB allows the same invoice to be reused:
/// 1. Create invoice for 300 tokens
/// 2. First payment: 300 tokens
/// 3. Second payment: same invoice, another 300 tokens
/// 4. Both payments should succeed
/// 5. Receiver gets 600 tokens total (300 + 300)
///
/// Note: RGB protocol allows invoice reuse, unlike Lightning Network
#[tokio::test]
#[serial(parallel)]
async fn test_invoice_reuse() {
    println!("\n=== Test: Invoice Reuse ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create two wallets
    println!("Creating wallets...");
    let wlt_1 = create_test_wallet("invoice_reuse_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("invoice_reuse_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");
    println!("✓ Wallets created");

    // Fund wallet 1 with multiple UTXOs for multiple allocations
    println!("\nFunding wallet 1...");
    let utxo1 = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO 1");
    let utxo2 = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO 2");
    println!("✓ Wallet 1 funded with 2 UTXOs");

    // Issue asset with 900 tokens on two UTXOs (500 + 400)
    let total_supply = 900u64;
    println!("\nIssuing NIA asset (supply: {})...", total_supply);
    let mut params = NIAIssueParams::new("InvoiceReuseAsset", "REUSE", "centiMilli", total_supply);
    params.add_allocation(utxo1, 500);
    params.add_allocation(utxo2, 400);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);

    // Share contract with wallet 2
    share_contract(&wlt_1, &wlt_2, contract_id, "InvoiceReuseAsset")
        .await
        .expect("Failed to share contract");
    println!("✓ Contract shared");

    // Create invoice for 300 tokens (Blinded type)
    let invoice_amount = 300u64;
    println!("\nCreating invoice for {} tokens...", invoice_amount);

    // Prepare blinding UTXO for wallet 2
    let blinding_utxo = get_utxo(&wlt_2, Some(5_000), INSTANCE_1)
        .await
        .expect("Failed to get blinding UTXO");

    let invoice = wlt_2
        .create_invoice(
            contract_id,
            invoice_amount,
            false,               // use_witness_utxo = false (Blinded)
            None,                // nonce
            Some(blinding_utxo), // blinding_utxo
        )
        .await;
    println!("✓ Invoice created");

    // First payment: 300 tokens
    println!("\n--- First Payment (reusing invoice) ---");
    let (consignment_key_1, tx_1, _payment_1) = wlt_1
        .transfer(invoice.clone(), Some(2000), None, true)
        .await
        .expect("First transfer failed");
    println!("✓ First transfer executed, tx: {}", tx_1.txid());

    // Confirm and sync
    chain::mine(false);
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ First transaction confirmed");

    // Accept first transfer
    wlt_2
        .accept_transfer(consignment_key_1)
        .await
        .expect("Failed to accept first transfer");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ First transfer accepted");

    // Second payment: same invoice, another 300 tokens
    println!("\n--- Second Payment (reusing SAME invoice) ---");
    let (consignment_key_2, tx_2, _payment_2) = wlt_1
        .transfer(invoice.clone(), Some(2000), None, true)
        .await
        .expect("Second transfer failed");
    println!("✓ Second transfer executed, tx: {}", tx_2.txid());

    // Confirm and sync
    chain::mine(false);
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ Second transaction confirmed");

    // Accept second transfer
    wlt_2
        .accept_transfer(consignment_key_2)
        .await
        .expect("Failed to accept second transfer");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ Second transfer accepted");

    // Verify final allocations
    println!("\nVerifying final allocations...");

    // Wallet 1 should have: 900 - 300 - 300 = 300 tokens
    // May be split as [200, 100] or merged as [300] by RGB runtime
    let expected_wlt_1_total = total_supply - invoice_amount - invoice_amount;
    check_allocations_sum(&wlt_1, contract_id, expected_wlt_1_total)
        .await
        .expect("Wallet 1 allocation sum check failed");
    println!("✓ Wallet 1: {} tokens", expected_wlt_1_total);

    // Wallet 2 should have: 300 + 300 = 600 tokens
    // May be two separate allocations [300, 300] or merged as [600] by RGB runtime
    let expected_wlt_2_total = invoice_amount + invoice_amount;
    check_allocations_sum(&wlt_2, contract_id, expected_wlt_2_total)
        .await
        .expect("Wallet 2 allocation sum check failed");
    println!("✓ Wallet 2: {} tokens (300 + 300)", invoice_amount * 2);

    println!("\n=== Test Passed ===");
    println!("  Invoice reuse works correctly");
    println!("  Both payments succeeded with same invoice");

    // Cleanup
    cleanup_test_wallet("invoice_reuse_wlt_1").await.ok();
    cleanup_test_wallet("invoice_reuse_wlt_2").await.ok();
}

/// Test paying one invoice twice (concurrent payments)
///
/// This test verifies concurrent payments to the same invoice:
/// 1. Create invoice for 100 tokens
/// 2. Create TWO transfer transactions for the same invoice (without broadcasting)
/// 3. Mine both transactions
/// 4. Accept both transfers
/// 5. Both should succeed, receiver gets 200 tokens total
///
/// Note: This tests RGB's ability to handle concurrent payments
#[tokio::test]
#[serial(parallel)]
async fn test_pay_invoice_twice() {
    println!("\n=== Test: Pay One Invoice Twice ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create two wallets
    println!("Creating wallets...");
    let wlt_1 = create_test_wallet("pay_twice_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("pay_twice_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");
    println!("✓ Wallets created");

    // Fund and issue asset
    println!("\nFunding and issuing asset...");
    let utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO");

    let issued_supply = 2000u64;
    let mut params = NIAIssueParams::new("PayTwiceAsset", "PAY2", "centiMilli", issued_supply);
    params.add_allocation(utxo, issued_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);

    // Share contract
    share_contract(&wlt_1, &wlt_2, contract_id, "PayTwiceAsset")
        .await
        .expect("Failed to share contract");
    println!("✓ Contract shared");

    // Create invoice for 100 tokens (Blinded type)
    let invoice_amount = 100u64;
    println!("\nCreating invoice for {} tokens...", invoice_amount);

    let blinding_utxo = get_utxo(&wlt_2, Some(5_000), INSTANCE_1)
        .await
        .expect("Failed to get blinding UTXO");

    let invoice = wlt_2
        .create_invoice(
            contract_id,
            invoice_amount,
            false,               // Blinded
            None,
            Some(blinding_utxo),
        )
        .await;
    println!("✓ Invoice created");

    // Create FIRST transfer (don't broadcast yet to simulate concurrent creation)
    println!("\n--- Creating First Transfer ---");
    let (consignment_key_1, tx_1, _payment_1) = wlt_1
        .transfer(invoice.clone(), Some(2000), None, true) // broadcast = false
        .await
        .expect("First transfer failed");
    println!("✓ First transfer created (not broadcasted), tx: {}", tx_1.txid());

    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");

    // Create SECOND transfer with same invoice (concurrent)
    println!("\n--- Creating Second Transfer (concurrent) ---");
    let (consignment_key_2, tx_2, _payment_2) = wlt_1
        .transfer(invoice.clone(), Some(2000), None, true) 
        .await
        .expect("Second transfer failed");
    println!("✓ Second transfer created (not broadcasted), tx: {}", tx_2.txid());

    dbg!(&consignment_key_1, &consignment_key_2);

    // Accept first transfer
    wlt_2
        .accept_transfer(consignment_key_1)
        .await
        .expect("Failed to accept first transfer");
    println!("✓ First transfer accepted");

    check_allocations(&wlt_2, contract_id, vec![invoice_amount])
        .await
        .expect("Wallet 2 allocation check failed after first payment");
    println!("✓ Balances correct after first payment");

    // Mine second transaction
    println!("\n--- Processing Second Payment ---");
    chain::mine(false);
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");
    println!("✓ Second transaction confirmed");

    // Accept second transfer
    wlt_2
        .accept_transfer(consignment_key_2)
        .await
        .expect("Failed to accept second transfer");
    println!("✓ Second transfer accepted");

    // Verify final allocations
    println!("\nVerifying final allocations...");

    // Wallet 1: 2000 - 100 - 100 = 1800
    let expected_wlt_1 = issued_supply - invoice_amount - invoice_amount;
    check_allocations(&wlt_1, contract_id, vec![expected_wlt_1])
        .await
        .expect("Wallet 1 final allocation check failed");
    println!("✓ Wallet 1: {} tokens", expected_wlt_1);

    // Wallet 2: 100 + 100 = 200 (two allocations)
    check_allocations(&wlt_2, contract_id, vec![invoice_amount, invoice_amount])
        .await
        .expect("Wallet 2 final allocation check failed");
    println!("✓ Wallet 2: {} tokens (100 + 100)", invoice_amount * 2);

    println!("\n=== Test Passed ===");
    println!("  Both concurrent payments succeeded");
    println!("  Invoice was paid twice successfully");

    // Cleanup
    cleanup_test_wallet("pay_twice_wlt_1").await.ok();
    cleanup_test_wallet("pay_twice_wlt_2").await.ok();
}

/// Test spending from UTXO with multiple RGB allocations
/// 
/// Wallet 场景: 同一UTXO对应同一资产的多个allocations, 花费任意allocation是否可以正常找零
///
/// Lightning 场景: 通道关闭时，多个 HTLC-Success TX 的 RGB 资产聚合到一个 UTXO。
/// 例如: HTLC_1(100) + HTLC_2(200) + HTLC_3(300) → 单个 UTXO [100,200,300]
///
/// 测试验证: 从多 allocation UTXO 中花费部分资产时，找零计算正确。
/// Setup: UTXO 有 [800, 200] → 花费 400 → 剩余 600 (800+200-400)
#[tokio::test]
#[serial(parallel)]
async fn test_spend_multiple_allocations_utxo() {
    println!("\n=== Test: Spend from UTXO with Multiple Allocations ===\n");

    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create three wallets
    println!("Creating wallets...");
    let wlt_1 = create_test_wallet("multi_alloc_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let wlt_2 = create_test_wallet("multi_alloc_wlt_2", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 2");

    let wlt_3 = create_test_wallet("multi_alloc_wlt_3", network, indexer_url)
        .await
        .expect("Failed to create wallet 3");
    println!("✓ Wallets created");

    // Phase 1: Preparation - Create UTXO with multiple allocations

    // 1.1. Wallet 1 issues asset
    println!("\n--- Phase 1.1: Asset Issuance ---");
    let issue_supply = 1200u64;
    let utxo = get_utxo(&wlt_1, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO");

    let mut params = NIAIssueParams::new("MultiAllocAsset", "MULTI", "centiMilli", issue_supply);
    params.add_allocation(utxo, issue_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");
    println!("✓ Asset issued: {}", contract_id);
    println!("  Initial supply: {} tokens", issue_supply);

    // Share contract with wallet 2 and 3
    share_contract(&wlt_1, &wlt_2, contract_id, "MultiAllocAsset")
        .await
        .expect("Failed to share contract with wallet 2");
    share_contract(&wlt_1, &wlt_3, contract_id, "MultiAllocAsset")
        .await
        .expect("Failed to share contract with wallet 3");
    println!("✓ Contract shared with wallet 2 and 3");

    // 1.2. Wallet 2 prepares a receiving UTXO
    println!("\n--- Phase 1.2: Prepare Receiving UTXO ---");
    let receiving_utxo = get_utxo(&wlt_2, Some(10_000), INSTANCE_1)
        .await
        .expect("Failed to get receiving UTXO");
    println!("✓ Wallet 2 receiving UTXO: {}", receiving_utxo);

    // 1.3. Wallet 1 sends 800 tokens to wallet 2's UTXO (first allocation)
    println!("\n--- Phase 1.3a: First Transfer (800 tokens) ---");
    let first_amount = 800u64;
    let invoice1 = wlt_2
        .create_invoice(
            contract_id,
            first_amount,
            false,                  // Blinded
            None,
            Some(receiving_utxo),   // Use specific UTXO
        )
        .await;

    let (consignment_key_1, tx_1, _) = wlt_1
        .transfer(invoice1, Some(2000), None, true)
        .await
        .expect("First transfer failed");
    println!("✓ First transfer executed, tx: {}", tx_1.txid());

    // Confirm and accept
    chain::mine(false);
    wlt_1.sync_runtime().await.unwrap();
    wlt_2.sync_runtime().await.unwrap();
    wlt_2.accept_transfer(consignment_key_1).await.unwrap();
    wlt_2.sync_runtime().await.unwrap();
    println!("✓ First transfer confirmed and accepted");

    // Verify wallet 2 has first allocation
    check_allocations(&wlt_2, contract_id, vec![first_amount])
        .await
        .expect("Wallet 2 should have first allocation");
    println!("✓ Wallet 2 has first allocation: {} tokens", first_amount);

    // 1.3b. Wallet 1 sends 200 tokens to the SAME UTXO (second allocation)
    println!("\n--- Phase 1.3b: Second Transfer (200 tokens to SAME UTXO) ---");
    let second_amount = 200u64;
    let invoice2 = wlt_2
        .create_invoice(
            contract_id,
            second_amount,
            false,                  // Blinded
            None,
            Some(receiving_utxo),   // SAME UTXO!
        )
        .await;

    let (consignment_key_2, tx_2, _) = wlt_1
        .transfer(invoice2, Some(2000), None, true)
        .await
        .expect("Second transfer failed");
    println!("✓ Second transfer executed, tx: {}", tx_2.txid());

    // Confirm and accept
    chain::mine(false);
    wlt_1.sync_runtime().await.unwrap();
    wlt_2.sync_runtime().await.unwrap();
    wlt_2.accept_transfer(consignment_key_2).await.unwrap();
    wlt_2.sync_runtime().await.unwrap();
    println!("✓ Second transfer confirmed and accepted");

    // 1.4. Verify wallet 2 now has TWO allocations on the same UTXO
    println!("\n--- Phase 1.4: Verify Multiple Allocations ---");
    check_allocations(&wlt_2, contract_id, vec![first_amount, second_amount])
        .await
        .expect("Wallet 2 should have two allocations");
    println!("✓ Wallet 2 has TWO allocations: [800, 200]");

    // Verify wallet 1 has correct change
    let wlt_1_change = issue_supply - first_amount - second_amount;
    check_allocations(&wlt_1, contract_id, vec![wlt_1_change])
        .await
        .expect("Wallet 1 should have change");
    println!("✓ Wallet 1 has change: {} tokens", wlt_1_change);

    // Phase 2: Action - Spend from multi-allocation UTXO
    println!("\n--- Phase 2: Spend from Multi-Allocation UTXO ---");
    let spend_amount = 400u64;
    println!("Wallet 2 will send {} tokens to wallet 3", spend_amount);
    println!("  (from UTXO with allocations [800, 200])");

    let invoice3 = wlt_3
        .create_invoice(contract_id, spend_amount, true, None, None)
        .await;

    let (consignment_key_3, tx_3, _) = wlt_2
        .transfer(invoice3, Some(2000), None, true)
        .await
        .expect("Third transfer failed");
    println!("✓ Transfer from multi-allocation UTXO executed");
    println!("  tx: {}", tx_3.txid());

    // Confirm and accept
    chain::mine(false);
    wlt_2.sync_runtime().await.unwrap();
    wlt_3.sync_runtime().await.unwrap();
    wlt_3.accept_transfer(consignment_key_3).await.unwrap();
    wlt_3.sync_runtime().await.unwrap();
    println!("✓ Transfer confirmed and accepted");

    // Phase 3: Verification
    println!("\n--- Phase 3: Verification ---");

    // 3.1. Wallet 3 should have received 400
    check_allocations(&wlt_3, contract_id, vec![spend_amount])
        .await
        .expect("Wallet 3 should have received tokens");
    println!("✓ Wallet 3 received: {} tokens", spend_amount);

    // 3.2. Wallet 2 should have correct change (600 = 800 + 200 - 400)
    let expected_change = first_amount + second_amount - spend_amount;
    println!("\nCritical verification: Wallet 2 change");
    println!("  Expected: {} + {} - {} = {}",
             first_amount, second_amount, spend_amount, expected_change);

    let wlt_2_allocations = wlt_2
        .get_allocations(contract_id)
        .await
        .expect("Failed to get wallet 2 allocations");
    let wlt_2_total: u64 = wlt_2_allocations.iter().map(|(_, amount)| amount).sum();

    assert_eq!(wlt_2_total, expected_change,
               "Wallet 2 total should be {}", expected_change);
    println!("✓ Wallet 2 has correct remaining balance: {} tokens", wlt_2_total);

    // 3.3. Total supply conservation
    println!("\n--- Final Verification: Supply Conservation ---");
    let wlt_1_final: u64 = wlt_1
        .get_allocations(contract_id)
        .await
        .unwrap()
        .iter()
        .map(|(_, amt)| amt)
        .sum();
    let wlt_2_final = wlt_2_total;
    let wlt_3_final: u64 = wlt_3
        .get_allocations(contract_id)
        .await
        .unwrap()
        .iter()
        .map(|(_, amt)| amt)
        .sum();

    let total_final = wlt_1_final + wlt_2_final + wlt_3_final;

    println!("  Wallet 1 final: {}", wlt_1_final);
    println!("  Wallet 2 final: {}", wlt_2_final);
    println!("  Wallet 3 final: {}", wlt_3_final);
    println!("  Total final: {}", total_final);
    println!("  Original supply: {}", issue_supply);

    assert_eq!(total_final, issue_supply,
               "Total supply must be conserved");
    println!("✓ Supply conservation verified");

    println!("\n=== Test Passed ===");
    println!("  Successfully spent from UTXO with multiple allocations");
    println!("  Multi-allocation UTXO handling works correctly");

    // Cleanup
    cleanup_test_wallet("multi_alloc_wlt_1").await.ok();
    cleanup_test_wallet("multi_alloc_wlt_2").await.ok();
    cleanup_test_wallet("multi_alloc_wlt_3").await.ok();
}

/// Test Replace-By-Fee (RBF) for RGB transfers
///
/// Test Flow:
/// 1. Setup: Issue 600 tokens, share contract with Wallet2
/// 2. Phase 1: Create initial transfer (400 tokens, no broadcast)
///    - Receiver accepts consignment
///    - Verify allocations: Wallet2(400) (client-side validation)
///    - Verify blockchain height unchanged
/// 3. Phase 2: First RBF (broadcast tx_2 to mempool)
///    - Receiver accepts consignment
///    - Sync wallets (only tx_2 in mempool, Phase 1's tx_1 was never broadcasted)
///    - Verify allocations: Wallet1(200), Wallet2(400)
///    - Verify blockchain height unchanged
/// 4. Phase 3: Second RBF (broadcast tx_3 replacing tx_2) + immediate mining
///    - Receiver accepts consignment
///    - **Immediately mine 1 block** (avoid tx_2 and tx_3 coexisting in mempool)
///    - Sync wallets
///    - Verify allocations: Wallet1(200), Wallet2(400) (on-chain confirmed)
///    - Verify blockchain height +1
///
/// Note:
/// - Mining immediately after Phase 3 avoids triggering bp-wallet's bug:
///   When the same UTXO is spent by multiple RBF versions, Esplora indexer
///   credits the UTXO only once but debits repeatedly, causing negative balance error
///
/// Scenarios:
/// - In Lightning during chain congestion, use RBF to increase commitment TX or HTLC-timeout TX fee rate
/// - Can perform multiple RBF operations until transaction confirms
#[tokio::test]
#[serial(serial)]
async fn test_rbf_transfer() {
    println!("\n=== Test RBF Transfer (2x RBF) ===");

    // Initialize test environment
    chain::initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Setup: Create wallets and issue asset
    println!("\n=== Setup: Create wallets and issue asset ===");

    let wlt_1 = create_test_wallet("rbf_wlt_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");
    let wlt_2 = create_test_wallet("rbf_wlt_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");

    println!("✓ Wallets created");

    // Fund wallet 1 with UTXO for issuance
    let utxo = get_utxo(&wlt_1, Some(10_000_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO for issuance");
    println!("✓ Wallet 1 funded with issuance UTXO: {}", utxo);

    // Fund wallet 2 with UTXO for blinded invoice
    let wlt_2_utxo = get_utxo(&wlt_2, Some(5_000), INSTANCE_1)
        .await
        .expect("Failed to get UTXO for wallet 2");
    println!("✓ Wallet 2 funded with UTXO: {}", wlt_2_utxo);

    // Issue NIA asset
    let total_supply = 600u64;
    let mut params = NIAIssueParams::new("RBFTestAsset", "RBF", "centiMilli", total_supply);
    params.add_allocation(utxo, total_supply);

    let contract_id = wlt_1
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");

    println!("✓ Asset issued with contract_id: {}", contract_id);

    // Share contract with wallet 2
    share_contract(&wlt_1, &wlt_2, contract_id, "RBFTestAsset")
        .await
        .expect("Failed to share contract");
    println!("✓ Contract shared with wallet 2");

    // Wait for any background mining from previous tests to complete
    println!("  Waiting for blockchain to stabilize...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Record initial blockchain height
    let initial_height = get_height().await;
    println!("  Initial blockchain height: {}", initial_height);

    // Phase 1: Create initial transfer WITHOUT broadcasting
    println!("\n=== Phase 1: Create initial transfer (NO broadcast) ===");

    let invoice_amount = 400u64;
    let invoice = wlt_2
        .create_invoice(contract_id, invoice_amount, false, None, Some(wlt_2_utxo))
        .await;

    let initial_fee_rate = 1.0; // Low fee rate (sats/vB)
    let (consignment_key_1, tx_1, payment) = wlt_1
        .transfer(invoice.clone(), None, Some(initial_fee_rate), false) // Don't broadcast
        .await
        .expect("Failed to create initial transfer");

    let txid_1 = tx_1.txid();
    println!("✓ Initial transfer created");
    println!("  Txid: {}", txid_1);
    println!("  Consignment key: {}", consignment_key_1);
    println!("  Status: NOT broadcasted");

    // Receiver accepts the consignment
    wlt_2
        .accept_transfer(consignment_key_1)
        .await
        .expect("Failed to accept initial consignment");
    println!("✓ Receiver accepted consignment");

    // Define expected allocations
    let expected_sender = total_supply - invoice_amount; // 200
    let expected_receiver = invoice_amount; // 400

    // Note: Wallet 1 (sender) allocation won't change yet because transaction is not broadcasted
    // Only wallet 2 (receiver) can see the allocation after accepting consignment
    check_allocations_sum(&wlt_2, contract_id, expected_receiver)
        .await
        .expect("Phase 1: Receiver allocation check failed");
    println!("✓ Receiver allocation verified (client-side): Wallet2({})", expected_receiver);
    println!("  Note: Sender allocation unchanged (transaction not broadcasted)");

    // Verify blockchain height unchanged
    let height_after_phase1 = get_height().await;
    assert_eq!(
        initial_height, height_after_phase1,
        "Phase 1: Height should not change (no broadcast)"
    );
    println!("✓ Blockchain height unchanged: {}", height_after_phase1);

    // Phase 2: First RBF (broadcast to mempool)
    println!("\n=== Phase 2: First RBF (BROADCAST to mempool) ===");

    let rbf_fee_1 = 2500u64; // Higher fee (sats)
    let (consignment_key_2, tx_2) = wlt_1
        .transfer_rbf(contract_id, payment.clone(), rbf_fee_1) 
        .await
        .expect("Failed to perform first RBF");

    let txid_2 = tx_2.txid();
    println!("✓ First RBF created");
    println!("  Txid: {}", txid_2);
    println!("  Consignment key: {}", consignment_key_2);
    println!("  Fee: {} sats", rbf_fee_1);
    println!("  Status: Broadcasted to mempool");
    assert_ne!(txid_1, txid_2, "RBF should create different txid");

    // Receiver accepts the new consignment
    wlt_2
        .accept_transfer(consignment_key_2)
        .await
        .expect("Failed to accept first RBF consignment");
    println!("✓ Receiver accepted first RBF consignment");

    // Sync wallets to see mempool state
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");

    // Verify allocations (mempool state)
    check_allocations_sum(&wlt_1, contract_id, expected_sender)
        .await
        .expect("Phase 2: Sender allocation check failed");
    check_allocations_sum(&wlt_2, contract_id, expected_receiver)
        .await
        .expect("Phase 2: Receiver allocation check failed");
    println!("✓ Allocations verified (mempool): Wallet1({}), Wallet2({})", expected_sender, expected_receiver);

    // Verify blockchain height unchanged (not mined yet)
    let height_after_phase2 = get_height().await;
    assert_eq!(
        initial_height, height_after_phase2,
        "Phase 2: Height should not change (not mined)"
    );
    println!("✓ Blockchain height unchanged: {} (waiting for mining)", height_after_phase2);

    // Phase 3: Second RBF + immediate mining (avoid multiple RBF versions in mempool)
    println!("\n=== Phase 3: Second RBF (BROADCAST with higher fee) + Mine ===");

    let rbf_fee_2 = 3000u64; // Even higher fee (sats)
    let (consignment_key_3, tx_3) = wlt_1
        .transfer_rbf(contract_id, payment.clone(), rbf_fee_2)
        .await
        .expect("Failed to perform second RBF");

    let txid_3 = tx_3.txid();
    println!("✓ Second RBF created");
    println!("  Txid: {}", txid_3);
    println!("  Consignment key: {}", consignment_key_3);
    println!("  Fee: {} sats", rbf_fee_2);
    println!("  Status: Broadcasted to mempool (replaces first RBF)");
    assert_ne!(txid_2, txid_3, "Second RBF should create different txid");

    // Receiver accepts the final consignment
    wlt_2
        .accept_transfer(consignment_key_3)
        .await
        .expect("Failed to accept second RBF consignment");
    println!("✓ Receiver accepted second RBF consignment");

    // ⚠️ IMPORTANT: Mine immediately after second RBF
    // This avoids having tx_2 and tx_3 coexisting in mempool, which would trigger
    // bp-wallet's bug where the same input UTXO gets debited twice but credited once
    println!("\n Mining immediately to avoid bp-wallet bug...");
    chain::mine(false);
    let final_height = get_height().await;
    assert_eq!(
        final_height,
        initial_height + 1,
        "Final height should be initial + 1"
    );
    println!("✓ Block mined, height: {} -> {}", initial_height, final_height);

    // Sync wallets to see confirmed state
    wlt_1.sync_runtime().await.expect("Failed to sync wallet 1");
    wlt_2.sync_runtime().await.expect("Failed to sync wallet 2");

    // Verify final allocations (on-chain confirmed)
    check_allocations_sum(&wlt_1, contract_id, expected_sender)
        .await
        .expect("Phase 3: Sender allocation check failed");
    check_allocations_sum(&wlt_2, contract_id, expected_receiver)
        .await
        .expect("Phase 3: Receiver allocation check failed");
    println!("✓ Final allocations (on-chain): Wallet1({}), Wallet2({})", expected_sender, expected_receiver);

    println!("\n=== Test Passed ===");
    println!("  Initial transfer: {} (not broadcasted)", txid_1);
    println!("  First RBF:        {} (fee: {} sats)", txid_2, rbf_fee_1);
    println!("  Second RBF:       {} (fee: {} sats)", txid_3, rbf_fee_2);
    println!("  Blockchain height: {} -> {}", initial_height, final_height);
    println!("  All 3 txids are different ✓");
    println!("  Only final RBF was confirmed on-chain ✓");

    // Cleanup
    cleanup_test_wallet("rbf_wlt_1").await.ok();
    cleanup_test_wallet("rbf_wlt_2").await.ok();
}
