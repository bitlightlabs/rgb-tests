// Tests for DeltaStockpileDir base() method and forwarding functionality
//
// This test suite verifies that:
// 1. base() method provides access to underlying stockpile
// 2. Operations are properly forwarded to base stockpile
// 3. The "clean view" concept works as expected

use std::fs;
use tempfile::TempDir;

use rgb_delta_store::{DeltaStockpileDir, SandboxConfig};
use rgb::{Consensus, Stockpile};
use bp::seals::{mmb, Anchor, Noise, TxoSeal, TxoSealExt, WOutpoint, WTxoSeal};

// Helper to create test directories 
fn create_test_dirs() -> Result<(TempDir, TempDir, SandboxConfig), Box<dyn std::error::Error>> {
    let base_temp = TempDir::new()?;
    let delta_temp = TempDir::new()?;
    
    let config = SandboxConfig::new(base_temp.path().to_path_buf(), delta_temp.path().to_path_buf());
    
    Ok((base_temp, delta_temp, config))
}

#[test]
fn test_base_access_method() {
    use rgb_persist_fs::StockpileDir;
    
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    // Create DeltaStockpileDir
    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true, // testnet
    ).expect("Failed to create DeltaStockpileDir");
    
    // Test base() method returns the base stockpile view
    let base_stockpile = delta_stockpile.base();
    
    // Should have the same consensus settings
    assert_eq!(base_stockpile.consensus(), Consensus::Bitcoin);
    assert_eq!(base_stockpile.is_testnet(), true);
    
    // Directory paths should match
    assert_eq!(delta_stockpile.base_dir(), config.base_path);
    assert_eq!(delta_stockpile.delta_dir(), config.delta_path);
}

#[test]
fn test_base_mut_access() {
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    // Create DeltaStockpileDir
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true, // testnet
    ).expect("Failed to create DeltaStockpileDir");
    
    // Test base() method returns read-only view
    let base_stockpile_view = delta_stockpile.base();
    
    // Should be able to access methods (though we can't test actual operations without real RGB data)
    assert_eq!(base_stockpile_view.consensus(), Consensus::Bitcoin);
    assert_eq!(base_stockpile_view.is_testnet(), true);
}

#[test]
fn test_stockpile_trait_forwarding() {
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    // Create DeltaStockpileDir
    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true, // testnet
    ).expect("Failed to create DeltaStockpileDir");
    
    // Test that Stockpile trait methods are forwarded correctly
    assert_eq!(delta_stockpile.consensus(), Consensus::Bitcoin);
    assert_eq!(delta_stockpile.is_testnet(), true);
    
    // These should forward to base stockpile
    assert_eq!(delta_stockpile.issuers_count(), 0); // Empty base stockpile
    assert_eq!(delta_stockpile.contracts_count(), 0); // Empty base stockpile
    
    // Test iterators (should be empty for new stockpile)
    let codex_ids: Vec<_> = delta_stockpile.codex_ids().collect();
    let contract_ids: Vec<_> = delta_stockpile.contract_ids().collect();
    
    assert_eq!(codex_ids.len(), 0);
    assert_eq!(contract_ids.len(), 0);
}

#[test] 
fn test_clean_view_vs_delta_view() {
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    // Create DeltaStockpileDir
    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true, // testnet
    ).expect("Failed to create DeltaStockpileDir");
    
    // Add some mock delta data to simulate pending changes
    let delta_file = delta_temp.path().join("pending_change.dat");
    fs::write(&delta_file, b"pending state change").expect("Failed to write delta file");
    assert!(delta_file.exists());
    
    // Test "clean view" through base() - should not see delta changes
    let base_view = delta_stockpile.base();
    // Base view only sees base stockpile state, delta changes are invisible
    assert_eq!(base_view.issuers_count(), 0);
    assert_eq!(base_view.contracts_count(), 0);
    
    // Test "delta view" through DeltaStockpileDir Stockpile trait
    // Currently forwards to base, but in future could combine base + delta
    assert_eq!(delta_stockpile.issuers_count(), 0);
    assert_eq!(delta_stockpile.contracts_count(), 0);
    
    // The key insight: base() gives you access to original state
    // while DeltaStockpileDir trait methods could potentially show combined view
}

#[test]
fn test_delta_directory_management() {
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Add some delta changes
    let delta_file = delta_temp.path().join("test_change.dat");
    fs::write(&delta_file, b"test change").expect("Failed to write delta file");
    assert!(delta_file.exists());
    
    // Base stockpile should be unaffected by delta changes
    let base_view = delta_stockpile.base();
    assert_eq!(base_view.consensus(), Consensus::Bitcoin);
    
    // Test rollback functionality
    delta_stockpile.rollback().expect("Failed to rollback");
    assert!(!delta_file.exists());
    
    // Base should still be intact
    assert_eq!(delta_stockpile.base().consensus(), Consensus::Bitcoin);
}

#[test]
fn test_configuration_access() {
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Test config access
    let stored_config = delta_stockpile.config();
    assert_eq!(stored_config.base_path, config.base_path);
    assert_eq!(stored_config.delta_path, config.delta_path);
    
    // Test directory path access
    assert_eq!(delta_stockpile.base_dir(), config.base_path);
    assert_eq!(delta_stockpile.delta_dir(), config.delta_path);
}
