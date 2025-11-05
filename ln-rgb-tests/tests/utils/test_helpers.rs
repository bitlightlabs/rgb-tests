// Test helper functions for ln-rgb-tests
//
// Provides utility functions to simplify test code

use super::*;
use bp::Outpoint;
use rgb::ContractId;
use std::sync::Arc;

/// Get a UTXO for the wallet by funding it with Bitcoin
///
/// This mirrors the behavior of rgb-tests TestWallet::get_utxo:
/// 1. Get a new address from the wallet
/// 2. Send Bitcoin to that address
/// 3. Wait for confirmation
/// 4. Sync wallet
/// 5. Return the funded UTXO outpoint
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `sats` - Amount of sats to fund (optional, defaults to 10_000)
/// * `instance` - Bitcoin node instance to use for funding
///
/// # Returns
/// The outpoint of the funded UTXO
pub async fn get_utxo(
    controller: &Arc<WalletController>,
    sats: Option<u64>,
    instance: u8,
) -> Result<Outpoint, String> {
    let sats = sats.unwrap_or(10_000);

    // 1. Get a new address from the wallet
    let address = controller.get_address().await?;
    println!("  Funding address: {}", address);

    // 2. Send Bitcoin to the address
    let txid = chain::fund_wallet(address.to_string(), Some(sats), instance);
    println!("  Funding txid: {}", txid);

    // 3. Wait for confirmation (fund_wallet already mines 1 block)
    assert!(
        chain::is_tx_confirmed(&txid, instance),
        "Funding transaction should be confirmed"
    );

    // 4. Sync wallet to see the new UTXO
    controller.sync_runtime().await?;

    // 5. Find and return the funded UTXO
    let utxos = controller.list_utxos().await?;
    println!("  Wallet has {} UTXOs after funding", utxos.len());

    // Find the UTXO from our funding transaction
    for utxo in utxos {
        let utxo_txid_str = utxo.outpoint.to_string();
        if utxo_txid_str.starts_with(&txid[..8]) {
            // Match by first 8 chars of txid
            println!("  Found funded UTXO: {}", utxo.outpoint);
            return Ok(utxo.outpoint);
        }
    }

    Err(format!(
        "Failed to find funded UTXO for txid {} in wallet",
        txid
    ))
}

/// Check if allocations match expected amounts
///
/// This is a simplified version of rgb-tests TestWallet::check_allocations
/// that only verifies amounts (sorted), not specific UTXOs.
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `contract_id` - The contract to check
/// * `expected` - Expected allocation amounts (will be sorted)
pub async fn check_allocations(
    controller: &Arc<WalletController>,
    contract_id: ContractId,
    mut expected: Vec<u64>,
) -> Result<(), String> {
    let allocations = controller.get_allocations(contract_id).await?;

    // Extract amounts and sort
    let mut actual: Vec<u64> = allocations.iter().map(|(_, amount)| *amount).collect();
    actual.sort();
    expected.sort();

    if actual == expected {
        println!("  ✓ Allocations match: {:?}", actual);
        Ok(())
    } else {
        Err(format!(
            "Allocation mismatch for contract {}: expected {:?}, got {:?}",
            contract_id, expected, actual
        ))
    }
}

/// Check if total allocation sum matches expected amount
///
/// This is useful when allocations may be merged/consolidated by the RGB runtime,
/// and we only care about the total balance, not the specific allocation distribution.
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `contract_id` - The contract to check
/// * `expected_sum` - Expected total amount
pub async fn check_allocations_sum(
    controller: &Arc<WalletController>,
    contract_id: ContractId,
    expected_sum: u64,
) -> Result<(), String> {
    let allocations = controller.get_allocations(contract_id).await?;

    // Calculate total sum
    let actual_sum: u64 = allocations.iter().map(|(_, amount)| *amount).sum();

    if actual_sum == expected_sum {
        println!("  ✓ Total allocation sum matches: {}", actual_sum);
        Ok(())
    } else {
        Err(format!(
            "Allocation sum mismatch for contract {}: expected {}, got {} (allocations: {:?})",
            contract_id,
            expected_sum,
            actual_sum,
            allocations.iter().map(|(_, amt)| amt).collect::<Vec<_>>()
        ))
    }
}

/// Get current blockchain height
///
/// Returns the current block count from the regtest bitcoin node
pub async fn get_height() -> u64 {
    super::chain::get_block_count().await
}

/// Share contract between two wallets
///
/// Exports the contract from sender's wallet and imports it to receiver's wallet.
/// Since both wallets use shared storage, this works seamlessly.
///
/// # Arguments
/// * `sender` - The wallet that issued the contract
/// * `receiver` - The wallet that will receive contract information
/// * `contract_id` - The contract to share
/// * `key` - Storage key for the contract (e.g., "TestAsset1")
pub async fn share_contract(
    sender: &Arc<WalletController>,
    receiver: &Arc<WalletController>,
    contract_id: ContractId,
    key: &str,
) -> Result<(), String> {
    println!("  Sharing contract {} with key '{}'", contract_id, key);

    // Export contract to shared storage
    sender.export_contract(contract_id, key.to_string()).await?;

    // Import contract from shared storage
    receiver
        .import_contract(contract_id, key.to_string())
        .await?;

    // Sync receiver's runtime
    receiver.sync_runtime().await?;

    println!("  ✓ Contract shared successfully");
    Ok(())
}
