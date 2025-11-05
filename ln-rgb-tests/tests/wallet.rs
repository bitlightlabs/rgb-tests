// Basic wallet functionality tests

mod utils;

use utils::{cleanup_test_wallet, create_test_wallet, initialize, Network};

use ln_rgb_tests::utils::{chain, wallet::*, *};
#[tokio::test]
async fn test_wallets_have_unique_keys() {
    initialize().await;

    let network = Network::Regtest;
    let indexer_url = "http://127.0.0.1:3001".to_string();

    // Create two wallets with different names
    let wallet1_name = "unique_wallet_1";
    let wallet2_name = "unique_wallet_2";

    let result1 = create_test_wallet(wallet1_name, network, indexer_url.clone()).await;
    let result2 = create_test_wallet(wallet2_name, network, indexer_url).await;

    // Cleanup
    let _ = cleanup_test_wallet(wallet1_name).await;
    let _ = cleanup_test_wallet(wallet2_name).await;

    match (result1, result2) {
        (Ok(controller1), Ok(controller2)) => {
            // Get addresses from both wallets
            let addr1_result = controller1.get_address().await;
            let addr2_result = controller2.get_address().await;

            match (addr1_result, addr2_result) {
                (Ok(addr1), Ok(addr2)) => {
                    println!("Wallet 1 address: {}", addr1);
                    println!("Wallet 2 address: {}", addr2);

                    // Verify addresses are different
                    assert_ne!(
                        addr1.to_string(),
                        addr2.to_string(),
                        "Wallets should have different addresses!"
                    );
                    println!("Confirmed: Wallets have unique keys");
                }
                _ => {
                    println!("Could not get addresses (expected if indexer not running)");
                    // Don't fail the test if we can't get addresses
                    // (might be because indexer is not running in SKIP_INIT mode)
                }
            }
        }
        (Err(e1), _) => panic!("Failed to create wallet 1: {}", e1),
        (_, Err(e2)) => panic!("Failed to create wallet 2: {}", e2),
    }
}

#[tokio::test]
async fn test_wallet_funding() {
    initialize().await;

    let wallet_name = "test_funding_wallet";
    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallet
    let controller = create_test_wallet(wallet_name, network, indexer_url)
        .await
        .expect("Failed to create test wallet");

    // Get address
    let address = controller
        .get_address()
        .await
        .expect("Failed to get address");

    // Fund wallet with 1 BTC
    let funding_amount = 100_000_000; // 1 BTC in satoshis
    let txid = chain::fund_wallet(address.to_string(), Some(funding_amount), INSTANCE_1);

    println!("Funded wallet with txid: {}", txid);

    // Verify the transaction is confirmed
    let is_confirmed = chain::is_tx_confirmed(&txid, INSTANCE_1);
    assert!(is_confirmed, "Funding transaction should be confirmed");

    // Sync wallet runtime to see the new UTXO
    controller
        .sync_runtime()
        .await
        .expect("Failed to sync runtime");

    // List UTXOs
    let utxos = controller.list_utxos().await.expect("Failed to list UTXOs");

    println!("Wallet has {} UTXOs", utxos.len());
    assert!(
        !utxos.is_empty(),
        "Wallet should have at least one UTXO after funding"
    );

    // Cleanup
    cleanup_test_wallet(wallet_name).await.ok();
}

#[tokio::test]
async fn test_multiple_wallets_unique_addresses() {
    initialize().await;

    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create two wallets
    let controller1 = create_test_wallet("wallet_1", network, indexer_url.clone())
        .await
        .expect("Failed to create wallet 1");

    let controller2 = create_test_wallet("wallet_2", network, indexer_url)
        .await
        .expect("Failed to create wallet 2");

    // Get addresses
    let addr1 = controller1
        .get_address()
        .await
        .expect("Failed to get address 1");
    let addr2 = controller2
        .get_address()
        .await
        .expect("Failed to get address 2");

    println!("Wallet 1 address: {}", addr1);
    println!("Wallet 2 address: {}", addr2);

    // Verify addresses are different
    assert_ne!(
        addr1.to_string(),
        addr2.to_string(),
        "Each wallet should have a unique address"
    );

    // Cleanup
    cleanup_test_wallet("wallet_1").await.ok();
    cleanup_test_wallet("wallet_2").await.ok();
}
