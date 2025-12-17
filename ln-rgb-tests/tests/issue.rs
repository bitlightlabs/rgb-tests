// RGB asset issuance tests for Lightning RGB wallet

use bpstd::Network;
use ln_rgb_tests::utils::{asset_params::NIAIssueParams, chain, wallet::*, *};

#[tokio::test]
async fn test_issue_nia_asset() {
    // Initialize test environment
    chain::initialize().await;

    let wallet_name = "test_issuer_wallet";
    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallet (with auto-import of issuers)
    let controller = create_test_wallet(wallet_name, network, indexer_url)
        .await
        .expect("Failed to create test wallet");

    // Get wallet address for funding
    let address = controller
        .get_address()
        .await
        .expect("Failed to get address");

    println!("Wallet address: {}", address);

    // Fund wallet with BTC (need UTXOs for RGB issuance)
    let funding_amount = 100_000_000; // 1 BTC
    let txid = chain::fund_wallet(address.to_string(), Some(funding_amount), INSTANCE_1);
    println!("Funded wallet with txid: {}", txid);

    // Wait for confirmation
    assert!(
        chain::is_tx_confirmed(&txid, INSTANCE_1),
        "Funding transaction should be confirmed"
    );

    // Sync wallet to see the new UTXO
    controller
        .sync_runtime()
        .await
        .expect("Failed to sync runtime");

    // Get UTXOs
    let utxos = controller.list_utxos().await.expect("Failed to list UTXOs");

    println!("Wallet has {} UTXOs", utxos.len());
    assert!(!utxos.is_empty(), "Wallet should have at least one UTXO");

    // Use the first UTXO as the allocation outpoint
    let allocation_outpoint = utxos[0];
    println!("Using allocation outpoint: {}:{}", allocation_outpoint.txid, allocation_outpoint.vout);

    // Create NIA issuance parameters
    let mut params = NIAIssueParams::new("TestToken", "TEST", "centiMilli", 1_000_000);
    params.add_allocation(allocation_outpoint, 1_000_000); // Allocate all supply to this UTXO

    println!("Issuing NIA asset with params:");
    println!("  Name: {}", params.name);
    println!("  Ticker: {}", params.ticker);
    println!("  Precision: {}", params.precision);
    println!("  Supply: {}", params.circulating_supply);

    // Issue the asset
    let contract_id = controller
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue asset");

    println!("Asset issued successfully!");
    println!("Contract ID: {}", contract_id);

    // Verify contract was created
    let contracts = controller
        .list_contracts()
        .await
        .expect("Failed to list contracts");

    println!("Wallet has {} contracts", contracts.len());
    assert!(
        contracts.contains(&contract_id),
        "Contract list should contain the newly issued contract"
    );

    // Get contract balance
    let balance = controller.get_balance(contract_id).await;

    println!("Contract balance: {:?}", balance);

    // Cleanup
    cleanup_test_wallet(wallet_name).await.ok();
}

#[tokio::test]
async fn test_issue_multiple_nia_assets() {
    chain::initialize().await;

    let wallet_name = "test_multi_issuer";
    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    let controller = create_test_wallet(wallet_name, network, indexer_url)
        .await
        .expect("Failed to create wallet");

    let address = controller
        .get_address()
        .await
        .expect("Failed to get address");

    // Fund wallet with multiple UTXOs
    for i in 0..3 {
        let txid = chain::fund_wallet(address.to_string(), Some(50_000_000), INSTANCE_1);
        println!("Funding tx {}: {}", i, txid);
        assert!(chain::is_tx_confirmed(&txid, INSTANCE_1));
    }

    controller.sync_runtime().await.expect("Failed to sync");

    let utxos = controller.list_utxos().await.expect("Failed to list UTXOs");
    println!("Wallet has {} UTXOs", utxos.len());
    assert!(utxos.len() >= 3, "Should have at least 3 UTXOs");

    // Issue 3 different assets
    let assets = vec![
        ("TokenA", "TKA", 1_000_000),
        ("TokenB", "TKB", 2_000_000),
        ("TokenC", "TKC", 5_000_000),
    ];

    let mut contract_ids = Vec::new();

    for (i, (name, ticker, supply)) in assets.iter().enumerate() {
        let mut params = NIAIssueParams::new(*name, *ticker, "centiMilli", *supply);
        params.add_allocation(utxos[i], *supply);

        println!(
            "Issuing asset {}: {} ({}) with supply {}",
            i, name, ticker, supply
        );

        let contract_id = controller
            .issue(params.into_create_params())
            .await
            .expect(&format!("Failed to issue asset {}", i));

        println!("  Contract ID: {}", contract_id);
        contract_ids.push(contract_id);
    }

    // Verify all contracts exist
    let contracts = controller
        .list_contracts()
        .await
        .expect("Failed to list contracts");

    println!("Total contracts in wallet: {}", contracts.len());
    assert_eq!(contracts.len(), 3, "Should have exactly 3 contracts");

    for contract_id in &contract_ids {
        assert!(
            contracts.contains(contract_id),
            "Contract {} should be in the list",
            contract_id
        );
    }

    cleanup_test_wallet(wallet_name).await.ok();
}

#[tokio::test]
async fn test_issue_nia_with_multiple_allocations() {
    chain::initialize().await;

    let wallet_name = "test_multi_allocation";
    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    let controller = create_test_wallet(wallet_name, network, indexer_url)
        .await
        .expect("Failed to create wallet");

    let address = controller
        .get_address()
        .await
        .expect("Failed to get address");

    // Fund with 3 UTXOs
    for _ in 0..3 {
        let txid = chain::fund_wallet(address.to_string(), Some(50_000_000), INSTANCE_1);
        assert!(chain::is_tx_confirmed(&txid, INSTANCE_1));
    }

    controller.sync_runtime().await.expect("Failed to sync");

    let utxos = controller.list_utxos().await.expect("Failed to list UTXOs");
    assert!(utxos.len() >= 3);

    // Create asset with multiple allocations
    let total_supply = 1_000_000u64;
    let mut params = NIAIssueParams::new("MultiAllocToken", "MAT", "centiMilli", total_supply);

    // Split supply across 3 UTXOs
    params.add_allocation(utxos[0], 300_000);
    params.add_allocation(utxos[1], 300_000);
    params.add_allocation(utxos[2], 400_000);

    println!("Issuing asset with 3 allocations:");
    println!("  UTXO 0: {}:{} -> 300,000", utxos[0].txid, utxos[0].vout);
    println!("  UTXO 1: {}:{} -> 300,000", utxos[1].txid, utxos[1].vout);
    println!("  UTXO 2: {}:{} -> 400,000", utxos[2].txid, utxos[2].vout);

    let contract_id = controller
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue multi-allocation asset");

    println!("Multi-allocation asset issued: {}", contract_id);

    // Verify contract exists
    let contracts = controller
        .list_contracts()
        .await
        .expect("Failed to list contracts");
    assert!(contracts.contains(&contract_id));

    // Get balance
    let balance = controller.get_balance(contract_id).await;
    println!("Contract balance: {:?}", balance);

    cleanup_test_wallet(wallet_name).await.ok();
}
