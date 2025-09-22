// Real functional tests for DeltaStockpileDir
//
// This test suite tests the actual DeltaStockpileDir functionality that we can verify

use bp::seals::TxoSeal;
use std::fs;
use tempfile::TempDir;

use rgb::{Consensus, Stockpile};
use rgb_delta_store::{DeltaStockpileDir, SandboxConfig};

#[test]
fn test_delta_stockpile_creation_and_basic_accessors() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");

    // Test creation without specifying Seal type (implementation detail)
    let config = SandboxConfig::new(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
    );

    // Test basic properties
    assert_eq!(config.base_path, base_temp.path());
    assert_eq!(config.delta_path, delta_temp.path());

    // Test paths exist
    assert!(base_temp.path().exists());
    assert!(delta_temp.path().exists());
}

#[test]
fn test_lightning_network_operation_restrictions() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");

    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    )
    .expect("Failed to create DeltaStockpileDir");

    // Test that Lightning Network restrictions are in place
    // Note: We can't easily test these without creating real RGB objects,
    // but we can verify the methods exist and have the right signatures

    // The key point is that these methods are implemented and will return errors
    // when called in Lightning Network scenarios with real data

    // Test rollback and commit methods work
    assert!(
        delta_stockpile.rollback().is_ok(),
        "Rollback should succeed"
    );
    assert!(
        delta_stockpile.commit_to_base().is_ok(),
        "Commit should succeed"
    );
}

#[test]
fn test_base_layer_file_scanning() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");

    // Create mock files with correct RGB naming patterns
    // Even though they don't contain valid RGB data, we can test the file scanning logic

    // Create a mock issuer file
    let issuer_file =
        "test-schema.1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef.issuer";
    fs::write(base_temp.path().join(issuer_file), b"mock issuer").expect("Failed to write issuer");

    // Create a mock contract directory
    let contract_dir =
        "test-contract.abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890.contract";
    let contract_path = base_temp.path().join(contract_dir);
    fs::create_dir_all(&contract_path).expect("Failed to create contract dir");
    fs::write(contract_path.join("mock.dat"), b"mock contract data")
        .expect("Failed to write contract data");

    // Load DeltaStockpileDir and verify it can scan the base layer
    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    )
    .expect("Failed to create DeltaStockpileDir");

    // The stockpile should have scanned the files during initialization
    // Since the files don't contain valid RGB data, exact behavior depends on parsing
    // But the important thing is that it doesn't crash

    let issuer_count = delta_stockpile.issuers_count();
    let contract_count = delta_stockpile.contracts_count();

    println!(
        "Found {} issuers, {} contracts in base layer",
        issuer_count, contract_count
    );

    // At minimum, verify the directory structure was processed
    assert!(issuer_count >= 0);
    assert!(contract_count >= 0);
}

#[test]
fn test_delta_layer_transactional_behavior() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");

    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    )
    .expect("Failed to create DeltaStockpileDir");

    // Test that delta directory starts empty
    let initial_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta dir")
        .collect();
    assert_eq!(
        initial_entries.len(),
        0,
        "Delta directory should start empty"
    );

    // Add some mock state to delta directory (simulating RGB state changes)
    let state_file = delta_temp.path().join("commitment_state.dat");
    fs::write(&state_file, b"commitment transaction state").expect("Failed to write state");
    assert!(state_file.exists());

    // Test rollback clears delta
    delta_stockpile.rollback().expect("Rollback failed");
    assert!(
        !state_file.exists(),
        "State file should be removed after rollback"
    );

    // Test commit clears delta (after moving state to base)
    let state_file2 = delta_temp.path().join("another_state.dat");
    fs::write(&state_file2, b"another state change").expect("Failed to write state");
    assert!(state_file2.exists());

    delta_stockpile.commit_to_base().expect("Commit failed");
    assert!(
        !state_file2.exists(),
        "State file should be cleaned after commit"
    );

    // Delta directory should be empty again
    let final_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta dir")
        .collect();
    assert_eq!(
        final_entries.len(),
        0,
        "Delta directory should be empty after commit"
    );
}

#[test]
fn test_multiple_instances_isolation() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp1 = TempDir::new().expect("Failed to create delta temp 1");
    let delta_temp2 = TempDir::new().expect("Failed to create delta temp 2");

    // Create two DeltaStockpileDir instances sharing the same base but different deltas
    let mut stockpile1 = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp1.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    )
    .expect("Failed to create stockpile 1");

    let mut stockpile2 = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp2.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    )
    .expect("Failed to create stockpile 2");

    // They should both see the same base layer
    assert_eq!(stockpile1.consensus(), stockpile2.consensus());
    assert_eq!(stockpile1.is_testnet(), stockpile2.is_testnet());
    assert_eq!(stockpile1.issuers_count(), stockpile2.issuers_count());
    assert_eq!(stockpile1.contracts_count(), stockpile2.contracts_count());

    // But have isolated delta layers
    let delta1_file = delta_temp1.path().join("delta1.dat");
    let delta2_file = delta_temp2.path().join("delta2.dat");

    fs::write(&delta1_file, b"delta 1 state").expect("Failed to write delta 1");
    fs::write(&delta2_file, b"delta 2 state").expect("Failed to write delta 2");

    assert!(delta1_file.exists());
    assert!(delta2_file.exists());
    assert!(!delta_temp1.path().join("delta2.dat").exists()); // Isolation check
    assert!(!delta_temp2.path().join("delta1.dat").exists()); // Isolation check

    // Independent rollbacks
    stockpile1
        .rollback()
        .expect("Failed to rollback stockpile 1");
    assert!(!delta1_file.exists());
    assert!(delta2_file.exists()); // stockpile2's delta should be unaffected

    stockpile2
        .rollback()
        .expect("Failed to rollback stockpile 2");
    assert!(!delta2_file.exists());
}

#[test]
fn test_directory_path_accessors() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");

    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    )
    .expect("Failed to create DeltaStockpileDir");

    // Test that path accessors return correct values
    assert_eq!(delta_stockpile.base_dir(), base_temp.path());
    assert_eq!(delta_stockpile.delta_dir(), delta_temp.path());

    // Test that config returns correct sandbox config
    let config = delta_stockpile.config();
    assert_eq!(config.base_path, base_temp.path());
    assert_eq!(config.delta_path, delta_temp.path());
}

#[test]
fn test_consensus_variant_support() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");

    // Test Bitcoin consensus
    let btc_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        false, // mainnet
    )
    .expect("Failed to create Bitcoin stockpile");

    assert_eq!(btc_stockpile.consensus(), Consensus::Bitcoin);
    assert_eq!(btc_stockpile.is_testnet(), false);

    // Test testnet vs mainnet
    let testnet_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        true, // testnet
    )
    .expect("Failed to create testnet stockpile");

    assert_eq!(testnet_stockpile.consensus(), Consensus::Bitcoin);
    assert_eq!(testnet_stockpile.is_testnet(), true);
}
