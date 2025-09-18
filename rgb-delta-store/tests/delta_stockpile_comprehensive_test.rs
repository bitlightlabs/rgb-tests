// Comprehensive tests for DeltaStockpileDir addressing missing test coverage
//
// This test suite covers:
// 1. commit_to_base() functionality 
// 2. Real data interaction tests
// 3. Error handling and edge cases
// 4. Future implementation scenarios

use std::fs;
use std::path::Path;
use tempfile::TempDir;

use rgb_delta_store::{DeltaStockpileDir, SandboxConfig};
use rgb::{Consensus, Stockpile};
use bp::seals::TxoSeal;

// Helper to create test directories 
fn create_test_dirs() -> Result<(TempDir, TempDir, SandboxConfig), Box<dyn std::error::Error>> {
    let base_temp = TempDir::new()?;
    let delta_temp = TempDir::new()?;
    
    let config = SandboxConfig::new(base_temp.path().to_path_buf(), delta_temp.path().to_path_buf());
    
    Ok((base_temp, delta_temp, config))
}

/// Test the main missing piece: commit_to_base() functionality
#[test]
fn test_commit_to_base_functionality() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Create some mock delta changes
    let delta_file1 = delta_temp.path().join("state_change1.dat");
    let delta_file2 = delta_temp.path().join("state_change2.dat");
    let delta_subdir = delta_temp.path().join("subdir");
    fs::create_dir_all(&delta_subdir).expect("Failed to create delta subdir");
    let delta_file3 = delta_subdir.join("nested_change.dat");
    
    fs::write(&delta_file1, b"delta change 1").expect("Failed to write delta file 1");
    fs::write(&delta_file2, b"delta change 2").expect("Failed to write delta file 2");
    fs::write(&delta_file3, b"nested delta change").expect("Failed to write nested delta file");
    
    // Verify delta changes exist before commit
    assert!(delta_file1.exists());
    assert!(delta_file2.exists());
    assert!(delta_file3.exists());
    assert!(delta_subdir.exists());
    
    // Commit changes to base
    delta_stockpile.commit_to_base().expect("Failed to commit to base");
    
    // After commit: delta directory should be cleaned but still exist
    assert!(delta_temp.path().exists(), "Delta directory should still exist");
    assert!(!delta_file1.exists(), "Delta file 1 should be removed after commit");
    assert!(!delta_file2.exists(), "Delta file 2 should be removed after commit");
    assert!(!delta_file3.exists(), "Nested delta file should be removed after commit");
    assert!(!delta_subdir.exists(), "Delta subdirectory should be removed after commit");
    
    // Delta directory should be empty
    let delta_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta directory")
        .collect();
    assert_eq!(delta_entries.len(), 0, "Delta directory should be empty after commit");
}

/// Test multiple commit/rollback cycles
#[test]
fn test_multiple_commit_rollback_cycles() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Cycle 1: Add changes, commit
    let file1 = delta_temp.path().join("cycle1.dat");
    fs::write(&file1, b"cycle 1 data").expect("Failed to write cycle 1 file");
    assert!(file1.exists());
    
    delta_stockpile.commit_to_base().expect("Failed to commit cycle 1");
    assert!(!file1.exists(), "Cycle 1 file should be removed after commit");
    
    // Cycle 2: Add changes, rollback
    let file2 = delta_temp.path().join("cycle2.dat");
    fs::write(&file2, b"cycle 2 data").expect("Failed to write cycle 2 file");
    assert!(file2.exists());
    
    delta_stockpile.rollback().expect("Failed to rollback cycle 2");
    assert!(!file2.exists(), "Cycle 2 file should be removed after rollback");
    
    // Cycle 3: Add changes, commit again
    let file3 = delta_temp.path().join("cycle3.dat");
    fs::write(&file3, b"cycle 3 data").expect("Failed to write cycle 3 file");
    assert!(file3.exists());
    
    delta_stockpile.commit_to_base().expect("Failed to commit cycle 3");
    assert!(!file3.exists(), "Cycle 3 file should be removed after commit");
    
    // Delta should remain functional after multiple cycles
    let final_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta directory")
        .collect();
    assert_eq!(final_entries.len(), 0, "Delta directory should be empty after final commit");
}

/// Test error handling: invalid paths and permissions
#[test]
fn test_error_handling_invalid_paths() {
    // Test with non-existent base directory
    let base_nonexistent = Path::new("/nonexistent/path/base");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    let result = DeltaStockpileDir::<TxoSeal>::load(
        base_nonexistent.to_path_buf(),
        delta_temp.path().to_path_buf(),
        Consensus::Bitcoin,
        true,
    );
    
    // Should fail gracefully with io::Error
    assert!(result.is_err(), "Should fail with non-existent base directory");
    let error = result.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

/// Test error handling: commit/rollback on corrupted delta directory
#[test]
fn test_error_handling_corrupted_delta() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Remove delta directory to simulate corruption
    fs::remove_dir_all(delta_temp.path()).expect("Failed to remove delta directory");
    assert!(!delta_temp.path().exists(), "Delta directory should be removed");
    
    // Rollback should handle missing delta directory gracefully
    let rollback_result = delta_stockpile.rollback();
    
    // The current implementation should either:
    // 1. Succeed and recreate the directory, or 
    // 2. Fail with a filesystem error
    match rollback_result {
        Ok(_) => {
            // If successful, verify the directory was recreated
            assert!(delta_temp.path().exists(), "Delta directory should be recreated after successful rollback");
            assert!(delta_temp.path().is_dir(), "Delta path should be a directory");
        },
        Err(e) => {
            // If it fails, should be due to filesystem issues
            println!("Rollback failed as expected with error: {:?}", e);
            assert!(matches!(
                e.kind(), 
                std::io::ErrorKind::NotFound | 
                std::io::ErrorKind::PermissionDenied |
                std::io::ErrorKind::Other
            ), "Should fail with appropriate filesystem error, got: {:?}", e.kind());
        }
    }
}

/// Test commit/rollback on empty delta directory
#[test]
fn test_commit_rollback_empty_delta() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Verify delta directory is empty
    let entries_before: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta directory")
        .collect();
    assert_eq!(entries_before.len(), 0, "Delta directory should be empty initially");
    
    // Commit on empty delta should succeed
    delta_stockpile.commit_to_base().expect("Commit on empty delta should succeed");
    
    // Rollback on empty delta should succeed
    delta_stockpile.rollback().expect("Rollback on empty delta should succeed");
    
    // Delta directory should still exist and be empty
    assert!(delta_temp.path().exists());
    let entries_after: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta directory")
        .collect();
    assert_eq!(entries_after.len(), 0, "Delta directory should remain empty");
}

/// Test base stockpile access patterns
#[test]
fn test_base_stockpile_access_patterns() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Test that base() and base_mut() access the same underlying stockpile
    let base_ref = delta_stockpile.base();
    let base_consensus = base_ref.consensus();
    let base_testnet = base_ref.is_testnet();
    
    let base_mut_ref = delta_stockpile.base_mut();
    let base_mut_consensus = base_mut_ref.consensus();
    let base_mut_testnet = base_mut_ref.is_testnet();
    
    // Should have identical properties
    assert_eq!(base_consensus, base_mut_consensus);
    assert_eq!(base_testnet, base_mut_testnet);
    assert_eq!(base_consensus, Consensus::Bitcoin);
    assert_eq!(base_testnet, true);
    
    // Test DeltaStockpileDir Stockpile trait forwarding
    assert_eq!(delta_stockpile.consensus(), base_consensus);
    assert_eq!(delta_stockpile.is_testnet(), base_testnet);
}

/// Test future implementation scenarios (preparing for TODO items)
#[test]
fn test_future_delta_merging_placeholder() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Current behavior: all methods forward to base
    let base_issuers = delta_stockpile.base().issuers_count();
    let delta_view_issuers = delta_stockpile.issuers_count();
    
    // Currently these should be equal (forwarding behavior)
    assert_eq!(base_issuers, delta_view_issuers);
    
    // TODO: When delta merging is implemented, this test should be extended to:
    // 1. Add some issuers through delta_stockpile
    // 2. Verify that delta_stockpile.issuers_count() > base().issuers_count()
    // 3. Verify that base().issuers_count() remains unchanged
    // 4. After commit, verify both counts are equal again
    
    // For now, just ensure the forwarding works
    assert_eq!(delta_stockpile.contracts_count(), delta_stockpile.base().contracts_count());
    
    // Test iterator forwarding
    let base_codex_ids: Vec<_> = delta_stockpile.base().codex_ids().collect();
    let delta_codex_ids: Vec<_> = delta_stockpile.codex_ids().collect();
    assert_eq!(base_codex_ids, delta_codex_ids);
}

/// Test configuration consistency across operations
#[test]
fn test_configuration_consistency() {
    let (_base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Store initial configuration
    let initial_config = delta_stockpile.config().clone();
    let initial_base_dir = delta_stockpile.base_dir().to_path_buf();
    let initial_delta_dir = delta_stockpile.delta_dir().to_path_buf();
    
    // Perform some operations
    let test_file = delta_temp.path().join("test.dat");
    fs::write(&test_file, b"test data").expect("Failed to write test file");
    
    delta_stockpile.commit_to_base().expect("Failed to commit");
    
    // Configuration should remain unchanged
    assert_eq!(delta_stockpile.config(), &initial_config);
    assert_eq!(delta_stockpile.base_dir(), initial_base_dir);
    assert_eq!(delta_stockpile.delta_dir(), initial_delta_dir);
    
    // After rollback
    delta_stockpile.rollback().expect("Failed to rollback");
    
    // Configuration should still be unchanged
    assert_eq!(delta_stockpile.config(), &initial_config);
    assert_eq!(delta_stockpile.base_dir(), initial_base_dir);
    assert_eq!(delta_stockpile.delta_dir(), initial_delta_dir);
}

/// Test directory structure integrity
#[test]
fn test_directory_structure_integrity() {
    let (base_temp, delta_temp, config) = create_test_dirs()
        .expect("Failed to create test directories");
    
    let mut delta_stockpile = DeltaStockpileDir::<TxoSeal>::load(
        config.base_path.clone(),
        config.delta_path.clone(),
        Consensus::Bitcoin,
        true,
    ).expect("Failed to create DeltaStockpileDir");
    
    // Create a complex directory structure in delta
    let subdir1 = delta_temp.path().join("contracts");
    let subdir2 = delta_temp.path().join("issuers");
    let nested_subdir = subdir1.join("nested");
    
    fs::create_dir_all(&subdir1).expect("Failed to create subdir1");
    fs::create_dir_all(&subdir2).expect("Failed to create subdir2");
    fs::create_dir_all(&nested_subdir).expect("Failed to create nested subdir");
    
    fs::write(subdir1.join("contract1.dat"), b"contract data").expect("Failed to write contract file");
    fs::write(subdir2.join("issuer1.dat"), b"issuer data").expect("Failed to write issuer file");
    fs::write(nested_subdir.join("nested.dat"), b"nested data").expect("Failed to write nested file");
    
    // Verify structure exists
    assert!(subdir1.exists());
    assert!(subdir2.exists());
    assert!(nested_subdir.exists());
    
    // Commit should clean all structures
    delta_stockpile.commit_to_base().expect("Failed to commit");
    
    // Verify all structures are cleaned
    assert!(!subdir1.exists(), "subdir1 should be removed");
    assert!(!subdir2.exists(), "subdir2 should be removed");
    assert!(!nested_subdir.exists(), "nested subdir should be removed");
    
    // But base and delta directories should still exist
    assert!(base_temp.path().exists(), "Base directory should still exist");
    assert!(delta_temp.path().exists(), "Delta directory should still exist");
    assert!(delta_temp.path().is_dir(), "Delta path should be a directory");
}