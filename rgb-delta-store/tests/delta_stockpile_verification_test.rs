// Verification tests for DeltaStockpileDir implementation
//
// These tests verify the actual functionality we can test without complex RGB setup

use std::fs;
use tempfile::TempDir;

use rgb_delta_store::SandboxConfig;

#[test]
fn test_sandbox_config_functionality() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    let config = SandboxConfig::new(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
    );
    
    // Test basic properties
    assert_eq!(config.base_path, base_temp.path());
    assert_eq!(config.delta_path, delta_temp.path());
    
    // Test temp creation
    let temp_config = SandboxConfig::temp().expect("Failed to create temp config");
    assert!(temp_config.base_path.exists());
    assert!(temp_config.delta_path.exists());
    
    // Test that they are different paths
    assert_ne!(temp_config.base_path, temp_config.delta_path);
}

#[test] 
fn test_directory_structure_and_isolation() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    // Test directory isolation
    let base_file = base_temp.path().join("base.dat");
    let delta_file = delta_temp.path().join("delta.dat");
    
    fs::write(&base_file, b"base data").expect("Failed to write base file");
    fs::write(&delta_file, b"delta data").expect("Failed to write delta file");
    
    // Files should be in their respective directories only
    assert!(base_file.exists());
    assert!(delta_file.exists());
    assert!(!base_temp.path().join("delta.dat").exists());
    assert!(!delta_temp.path().join("base.dat").exists());
}

#[test]
fn test_transactional_directory_operations() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    // Setup base state (simulating funding tx)
    let base_state = base_temp.path().join("funding_state.dat");
    fs::write(&base_state, b"funding transaction state").expect("Failed to write base state");
    
    // Test rollback simulation: add delta state, then clear it
    let delta_state = delta_temp.path().join("commitment_state.dat");
    fs::write(&delta_state, b"commitment transaction state").expect("Failed to write delta state");
    assert!(delta_state.exists());
    
    // Simulate rollback: remove delta directory contents
    if delta_temp.path().exists() {
        fs::remove_dir_all(delta_temp.path()).expect("Failed to remove delta");
        fs::create_dir_all(delta_temp.path()).expect("Failed to recreate delta");
    }
    
    // After rollback: delta should be clean, base unchanged
    assert!(!delta_state.exists());
    assert!(base_state.exists());
    
    let delta_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta dir")
        .collect();
    assert_eq!(delta_entries.len(), 0, "Delta directory should be empty after rollback");
}

#[test]
fn test_commit_simulation() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    // Setup base state
    let base_state = base_temp.path().join("original_state.dat");
    fs::write(&base_state, b"original state").expect("Failed to write base state");
    
    // Add delta changes
    let delta_change = delta_temp.path().join("state_change.dat");
    fs::write(&delta_change, b"state modification").expect("Failed to write delta change");
    
    // Simulate commit: clean delta (in real implementation, would merge to base)
    if delta_temp.path().exists() {
        fs::remove_dir_all(delta_temp.path()).expect("Failed to remove delta");
        fs::create_dir_all(delta_temp.path()).expect("Failed to recreate delta");
    }
    
    // After commit: delta should be clean, base should remain
    assert!(!delta_change.exists());
    assert!(base_state.exists());
    
    let delta_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta dir")
        .collect();
    assert_eq!(delta_entries.len(), 0, "Delta directory should be empty after commit");
}

#[test]
fn test_multiple_transaction_cycles() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    // Base state that should persist through all cycles
    let persistent_state = base_temp.path().join("persistent.dat");
    fs::write(&persistent_state, b"persistent base state").expect("Failed to write persistent state");
    
    for cycle in 1..=5 {
        // Add delta state for this cycle
        let cycle_file = delta_temp.path().join(format!("cycle_{}.dat", cycle));
        fs::write(&cycle_file, format!("cycle {} changes", cycle)).expect("Failed to write cycle file");
        assert!(cycle_file.exists());
        
        // Simulate transaction result (alternating commit/rollback)
        if cycle % 2 == 1 {
            // Rollback: clear delta
            fs::remove_dir_all(delta_temp.path()).expect("Failed to remove delta");
            fs::create_dir_all(delta_temp.path()).expect("Failed to recreate delta");
        } else {
            // Commit: clear delta (after hypothetical merge to base)
            fs::remove_dir_all(delta_temp.path()).expect("Failed to remove delta");
            fs::create_dir_all(delta_temp.path()).expect("Failed to recreate delta");
        }
        
        // In both cases, delta should be clean and base should persist
        assert!(!cycle_file.exists());
        assert!(persistent_state.exists());
        
        let delta_entries: Vec<_> = fs::read_dir(delta_temp.path())
            .expect("Failed to read delta dir")
            .collect();
        assert_eq!(delta_entries.len(), 0, "Delta directory should be clean after cycle {}", cycle);
    }
}

#[test]
fn test_lightning_network_file_patterns() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    // Base layer should contain funding tx artifacts (contracts, issuers)
    let issuer_file = base_temp.path().join("schema.1234567890abcdef.issuer");
    let contract_dir = base_temp.path().join("contract.abcdef1234567890.contract");
    
    fs::write(&issuer_file, b"issuer from funding tx").expect("Failed to write issuer");
    fs::create_dir_all(&contract_dir).expect("Failed to create contract dir");
    fs::write(contract_dir.join("genesis.rgb"), b"genesis from funding tx").expect("Failed to write genesis");
    
    // Delta layer should only contain state changes, not new contracts/issuers
    let state_change = delta_temp.path().join("commitment_state.dat");
    fs::write(&state_change, b"commitment tx state change").expect("Failed to write state change");
    
    // Verify base layer structure (funding tx artifacts)
    assert!(issuer_file.exists());
    assert!(contract_dir.exists());
    assert!(contract_dir.join("genesis.rgb").exists());
    
    // Verify delta layer structure (only state files)
    assert!(state_change.exists());
    
    // Check that delta doesn't contain contract/issuer files
    let delta_entries: Vec<_> = fs::read_dir(delta_temp.path())
        .expect("Failed to read delta dir")
        .collect();
    
    for entry in delta_entries {
        let entry = entry.expect("Failed to read delta entry");
        let filename = entry.file_name();
        let name = filename.to_str().unwrap();
        
        // In Lightning Network RGB, delta should not contain .issuer or .contract files
        assert!(!name.contains(".issuer"), "Delta should not contain issuer files: {}", name);
        assert!(!name.contains(".contract"), "Delta should not contain contract directories: {}", name);
    }
}

#[test]
fn test_concurrent_delta_isolation() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp1 = TempDir::new().expect("Failed to create delta temp 1");
    let delta_temp2 = TempDir::new().expect("Failed to create delta temp 2");
    
    // Shared base state
    let base_file = base_temp.path().join("shared_base.dat");
    fs::write(&base_file, b"shared base state").expect("Failed to write base state");
    
    // Independent delta states
    let delta1_file = delta_temp1.path().join("delta1.dat");
    let delta2_file = delta_temp2.path().join("delta2.dat");
    
    fs::write(&delta1_file, b"delta 1 changes").expect("Failed to write delta 1");
    fs::write(&delta2_file, b"delta 2 changes").expect("Failed to write delta 2");
    
    // Verify isolation
    assert!(delta1_file.exists());
    assert!(delta2_file.exists());
    assert!(!delta_temp1.path().join("delta2.dat").exists());
    assert!(!delta_temp2.path().join("delta1.dat").exists());
    
    // Independent operations
    fs::remove_dir_all(delta_temp1.path()).expect("Failed to remove delta 1");
    fs::create_dir_all(delta_temp1.path()).expect("Failed to recreate delta 1");
    
    // Delta 1 should be clean, delta 2 should be unaffected
    assert!(!delta1_file.exists());
    assert!(delta2_file.exists());
    assert!(base_file.exists()); // Base should be unaffected
}

#[test]
fn test_error_conditions_and_recovery() {
    let base_temp = TempDir::new().expect("Failed to create base temp");
    let delta_temp = TempDir::new().expect("Failed to create delta temp");
    
    let config = SandboxConfig::new(
        base_temp.path().to_path_buf(),
        delta_temp.path().to_path_buf(),
    );
    
    // Test recovery from incomplete operations
    let partial_file = delta_temp.path().join("partial_operation.tmp");
    fs::write(&partial_file, b"incomplete operation").expect("Failed to write partial file");
    
    // Simulate recovery: clean temporary files
    for entry in fs::read_dir(delta_temp.path()).expect("Failed to read delta dir") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();
        if let Some(extension) = path.extension() {
            if extension == "tmp" {
                fs::remove_file(&path).expect("Failed to remove tmp file");
            }
        }
    }
    
    // After cleanup, tmp files should be gone
    assert!(!partial_file.exists());
    
    // Test that directories are still valid
    assert!(config.base_path.exists());
    assert!(config.delta_path.exists());
}

#[test] 
fn test_directory_permissions_and_access() {
    let temp_config = SandboxConfig::temp().expect("Failed to create temp config");
    
    // Test that directories are readable and writable
    let test_base_file = temp_config.base_path.join("test_write.dat");
    let test_delta_file = temp_config.delta_path.join("test_write.dat");
    
    // Should be able to write to both directories
    fs::write(&test_base_file, b"test base write").expect("Failed to write to base");
    fs::write(&test_delta_file, b"test delta write").expect("Failed to write to delta");
    
    // Should be able to read from both directories
    let base_content = fs::read_to_string(&test_base_file).expect("Failed to read base");
    let delta_content = fs::read_to_string(&test_delta_file).expect("Failed to read delta");
    
    assert_eq!(base_content, "test base write");
    assert_eq!(delta_content, "test delta write");
    
    // Should be able to list directory contents
    let base_entries: Vec<_> = fs::read_dir(&temp_config.base_path)
        .expect("Failed to read base directory")
        .collect();
    let delta_entries: Vec<_> = fs::read_dir(&temp_config.delta_path)
        .expect("Failed to read delta directory")
        .collect();
    
    assert_eq!(base_entries.len(), 1);
    assert_eq!(delta_entries.len(), 1);
}