// Integration tests for RGB Sandbox Stockpile
//
// This test suite verifies the delta-over-base functionality of SandboxStock.
// Based on our analysis of RGB components (see docs/), we understand that:
//
// 1. Articles contain Semantics + Issue + optional signature
// 2. EffectiveState manages RawState + ProcessedState layers
// 3. StockFs integrates these components with file-based persistence
// 4. Our SandboxStock implements delta-over-base with two StockFs instances
//
// Testing Strategy:
// - Focus on sandbox logic and file system isolation
// - Use minimal RGB components where possible
// - Test directory setup, configuration, and basic sandbox behavior
// - Defer full RGB integration testing until proper component construction is available

use std::path::PathBuf;
use tempfile::TempDir;

// Note: Most RGB types commented out due to construction complexity
// These would be needed for full integration testing:
// use hypersonic::{Stock, Articles, EffectiveState, Operation, Opid, Transition};
// use sonic_persist_fs::StockFs;

use hypersonic::Stock;
use rgb_delta_store::{SandboxConfig, SandboxResult};

// These types are available for testing but require complex setup:
// - Articles: needs Semantics (Api + TypeSystem) + Issue (Meta + Codex + Genesis) + optional signature
// - EffectiveState: needs Articles for proper initialization via with_articles()
// - Operation: needs ContractId + CallId + valid StateValue components
// - StockFs: needs valid Articles + EffectiveState for construction

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_sandbox_config_creation() {
        let config = SandboxConfig::temp().expect("Failed to create temp config");

        // Verify paths are different
        assert_ne!(
            config.base_path, config.delta_path,
            "Base and delta paths must be different"
        );

        // Verify both paths are absolute
        assert!(config.base_path.is_absolute(), "Base path must be absolute");
        assert!(
            config.delta_path.is_absolute(),
            "Delta path must be absolute"
        );

        // Verify paths actually exist
        assert!(
            config.base_path.exists(),
            "Base path should exist after temp creation"
        );
        assert!(
            config.delta_path.exists(),
            "Delta path should exist after temp creation"
        );

        // Verify paths are directories
        assert!(config.base_path.is_dir(), "Base path should be a directory");
        assert!(
            config.delta_path.is_dir(),
            "Delta path should be a directory"
        );

        // Verify paths are writable
        let test_file_base = config.base_path.join("test_write.tmp");
        let test_file_delta = config.delta_path.join("test_write.tmp");

        std::fs::write(&test_file_base, "test").expect("Should be able to write to base path");
        std::fs::write(&test_file_delta, "test").expect("Should be able to write to delta path");

        // Cleanup test files
        std::fs::remove_file(test_file_base).ok();
        std::fs::remove_file(test_file_delta).ok();
    }

    #[test]
    fn test_sandbox_directory_isolation() {
        // Create explicit temporary directories
        let base_dir = TempDir::new().expect("Failed to create base temp dir");
        let delta_dir = TempDir::new().expect("Failed to create delta temp dir");

        let config = SandboxConfig::new(
            base_dir.path().to_path_buf(),
            delta_dir.path().to_path_buf(),
        );

        // Verify complete separation
        assert_eq!(
            config.base_path,
            base_dir.path(),
            "Base path should match the provided directory"
        );
        assert_eq!(
            config.delta_path,
            delta_dir.path(),
            "Delta path should match the provided directory"
        );
        assert_ne!(
            config.base_path, config.delta_path,
            "Base and delta paths must be different"
        );

        // Verify neither is a subdirectory of the other
        assert!(
            !config.base_path.starts_with(&config.delta_path),
            "Base path should not be a subdirectory of delta"
        );
        assert!(
            !config.delta_path.starts_with(&config.base_path),
            "Delta path should not be a subdirectory of base"
        );

        // Test content isolation with multiple files
        let base_files = [
            ("base_file.txt", "base data"),
            ("shared_name.txt", "base version"),
        ];
        let delta_files = [
            ("delta_file.txt", "delta data"),
            ("shared_name.txt", "delta version"),
        ];

        // Write files to both directories
        for (filename, content) in base_files.iter() {
            std::fs::write(config.base_path.join(filename), content)
                .expect(&format!("Should be able to write {} to base dir", filename));
        }

        for (filename, content) in delta_files.iter() {
            std::fs::write(config.delta_path.join(filename), content).expect(&format!(
                "Should be able to write {} to delta dir",
                filename
            ));
        }

        // Verify file existence and content isolation
        assert!(
            config.base_path.join("base_file.txt").exists(),
            "base_file.txt should exist in base"
        );
        assert!(
            config.delta_path.join("delta_file.txt").exists(),
            "delta_file.txt should exist in delta"
        );

        // Verify files don't cross-contaminate
        assert!(
            !config.base_path.join("delta_file.txt").exists(),
            "delta_file.txt should not exist in base"
        );
        assert!(
            !config.delta_path.join("base_file.txt").exists(),
            "base_file.txt should not exist in delta"
        );

        // Verify same filename can have different content in each directory
        let base_shared_content = std::fs::read_to_string(config.base_path.join("shared_name.txt"))
            .expect("Should be able to read shared_name.txt from base");
        let delta_shared_content =
            std::fs::read_to_string(config.delta_path.join("shared_name.txt"))
                .expect("Should be able to read shared_name.txt from delta");

        assert_eq!(
            base_shared_content, "base version",
            "Base version of shared file should have correct content"
        );
        assert_eq!(
            delta_shared_content, "delta version",
            "Delta version of shared file should have correct content"
        );
        assert_ne!(
            base_shared_content, delta_shared_content,
            "Same filename should have different content in isolated directories"
        );
    }

    #[test]
    fn test_sandbox_config_validation() {
        // Test 1: Same path scenario (edge case - should work but not typical)
        let same_dir = TempDir::new().expect("Failed to create temp dir");
        let same_path = same_dir.path().to_path_buf();

        let config_same = SandboxConfig::new(same_path.clone(), same_path.clone());
        assert_eq!(
            config_same.base_path, config_same.delta_path,
            "Same path config should have identical paths"
        );
        assert!(config_same.base_path.exists(), "Same path should exist");
        assert!(
            config_same.base_path.is_dir(),
            "Same path should be a directory"
        );

        // Test 2: Different paths (typical scenario)
        let config_different = SandboxConfig::temp().expect("Failed to create config");
        assert_ne!(
            config_different.base_path, config_different.delta_path,
            "Different path config should have different paths"
        );
        assert!(
            config_different.base_path.exists(),
            "Base path should exist in different config"
        );
        assert!(
            config_different.delta_path.exists(),
            "Delta path should exist in different config"
        );

        // Test 3: Manual path construction
        let base_temp = TempDir::new().expect("Failed to create base temp");
        let delta_temp = TempDir::new().expect("Failed to create delta temp");

        let config_manual = SandboxConfig::new(
            base_temp.path().to_path_buf(),
            delta_temp.path().to_path_buf(),
        );

        assert!(
            config_manual.base_path.is_absolute(),
            "Manual base path should be absolute"
        );
        assert!(
            config_manual.delta_path.is_absolute(),
            "Manual delta path should be absolute"
        );
        assert_ne!(
            config_manual.base_path, config_manual.delta_path,
            "Manual paths should be different"
        );

        // Test 4: Path canonicalization behavior
        let canonical_base = config_manual
            .base_path
            .canonicalize()
            .expect("Base path should be canonicalizable");
        let canonical_delta = config_manual
            .delta_path
            .canonicalize()
            .expect("Delta path should be canonicalizable");

        assert_eq!(
            canonical_base,
            base_temp.path().canonicalize().unwrap(),
            "Base path should canonicalize correctly"
        );
        assert_eq!(
            canonical_delta,
            delta_temp.path().canonicalize().unwrap(),
            "Delta path should canonicalize correctly"
        );
    }

    #[test]
    fn test_error_handling_coverage() {
        use rgb_delta_store::{SandboxConfig, SandboxError, SandboxStock};
        use std::fs;
        use std::path::PathBuf;

        // Test 1: Invalid base path handling
        let invalid_path = PathBuf::from("/nonexistent/invalid/path/that/should/not/exist");
        let config = SandboxConfig::new(
            invalid_path,
            tempfile::tempdir().unwrap().path().to_path_buf(),
        );

        // This should fail when trying to load from non-existent base
        let result = SandboxStock::load(config);
        assert!(
            result.is_err(),
            "Loading from invalid base path should fail"
        );

        // Test 2: Invalid delta path handling (insufficient permissions)
        let base_temp = tempfile::tempdir().expect("Failed to create base temp dir");
        let delta_temp = tempfile::tempdir().expect("Failed to create delta temp dir");
        let config = SandboxConfig::new(
            base_temp.path().to_path_buf(),
            delta_temp.path().to_path_buf(),
        );

        // Make delta path read-only to simulate permission issues
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(delta_temp.path()).unwrap().permissions();
            perms.set_mode(0o444); // read-only
            fs::set_permissions(delta_temp.path(), perms).unwrap();

            // Attempting to create sandbox with read-only delta should fail
            let articles = rgb_delta_store::rgb_components::create_rgb20_articles()
                .expect("Failed to create test articles");
            let state = rgb_delta_store::rgb_components::create_effective_state(&articles)
                .expect("Failed to create test state");

            // First create a base StockFs, then try to create sandbox (which should fail due to delta permissions)
            let base = rgb_delta_store::rgb_components::create_test_stock(
                articles.clone(),
                state.clone(),
                base_temp.path().to_path_buf(),
            )
            .expect("Should be able to create base stock");
            let result = SandboxStock::create_with_base(config, base);

            // This should fail due to permissions
            assert!(
                result.is_err(),
                "Creating sandbox with read-only delta should fail"
            );

            // Restore permissions for cleanup
            let mut restore_perms = fs::metadata(delta_temp.path()).unwrap().permissions();
            restore_perms.set_mode(0o755);
            fs::set_permissions(delta_temp.path(), restore_perms).unwrap();
        }

        // Test 3: DataNotFound error variant
        let error = SandboxError::DataNotFound;
        assert_eq!(
            format!("{}", error),
            "Data not found in either base or delta storage",
            "DataNotFound error should have correct display"
        );

        // Test 4: Error type structure verification
        match error {
            SandboxError::DataNotFound => {
                // Expected case
            }
            _ => panic!("DataNotFound should match correctly"),
        }
    }

    #[test]
    fn test_articles_construction() {
        use rgb_delta_store::rgb_components;

        // Test creating Articles using built-in RGB20 template (no external dependencies)
        let articles = rgb_components::create_rgb20_articles()
            .expect("Should be able to create RGB20 articles from built-in template");

        // Verify Articles structure and content
        assert!(
            !articles.genesis().version.to_string().is_empty(),
            "Articles should have a valid version"
        );

        // Verify the articles contain expected components
        let default_api = articles.default_api();
        assert!(
            !default_api.verifiers.is_empty(),
            "Articles should have API verifiers"
        );

        // Test EffectiveState creation from Articles
        let effective_state = rgb_components::create_effective_state(&articles)
            .expect("Should be able to create EffectiveState from valid Articles");

        // Verify EffectiveState was created successfully
        // Note: Initial RGB20 state may be empty, which is valid for a new contract
        // The important thing is that the state structure exists and is properly initialized

        // Test that we can create Operations from these Articles
        let operation = rgb_components::create_test_operation(&articles, None)
            .expect("Should be able to create test operation from Articles");

        // Verify operation has correct structure
        assert!(
            !operation.contract_id.to_string().is_empty(),
            "ContractId should have a non-empty string representation"
        );

        // Verify the CallId comes from Articles API
        let expected_call_id = default_api
            .verifiers
            .first_key_value()
            .map(|(_, call_id)| *call_id)
            .expect("Articles should have at least one API verifier");
        assert_eq!(
            operation.call_id, expected_call_id,
            "Operation CallId should match Articles API verifier"
        );
    }

    #[test]
    fn test_complete_rgb_integration() {
        use rgb_delta_store::rgb_components;

        // Step 1: Create Articles (no external dependencies)
        let articles = rgb_components::create_rgb20_articles()
            .expect("Should be able to create RGB20 articles");

        // Verify Articles creation
        assert!(
            !articles.genesis().version.to_string().is_empty(),
            "Articles should have valid version"
        );
        let api_verifier_count = articles.default_api().verifiers.len();
        assert!(
            api_verifier_count > 0,
            "Articles should have at least one API verifier"
        );

        // Step 2: Create EffectiveState
        let effective_state = rgb_components::create_effective_state(&articles)
            .expect("Should be able to create EffectiveState from Articles");

        // Verify EffectiveState creation succeeded
        // Note: Initial state may be empty for a new RGB20 contract, which is valid

        // Step 3: Create Operation
        let operation = rgb_components::create_test_operation(&articles, None)
            .expect("Should be able to create test operation");

        // Verify Operation creation and content
        assert!(
            !operation.contract_id.to_string().is_empty(),
            "ContractId should have a non-empty string representation"
        );

        // Verify CallId matches Articles API
        let expected_call_id = articles
            .default_api()
            .verifiers
            .first_key_value()
            .map(|(_, call_id)| *call_id)
            .expect("Articles should have API verifiers");
        assert_eq!(
            operation.call_id, expected_call_id,
            "Operation CallId should match Articles API"
        );

        // Step 4: Create StockFs
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let stock = rgb_components::create_test_stock(
            articles.clone(),
            effective_state.clone(),
            temp_dir.path().to_path_buf(),
        )
        .expect("Should be able to create StockFs with RGB components");

        // Verify StockFs creation and content
        assert_eq!(
            stock.articles().genesis().version,
            articles.genesis().version,
            "StockFs should preserve Articles version"
        );
        // Note: RawState doesn't implement PartialEq, so we verify individual fields
        assert_eq!(
            stock.state().raw.auth.len(),
            effective_state.raw.auth.len(),
            "StockFs should preserve auth state length"
        );
        assert_eq!(
            stock.state().raw.global.len(),
            effective_state.raw.global.len(),
            "StockFs should preserve global state length"
        );
        assert_eq!(
            stock.state().raw.owned.len(),
            effective_state.raw.owned.len(),
            "StockFs should preserve owned state length"
        );
        assert_eq!(
            stock.operation_count(),
            0,
            "New StockFs should start with 0 operations"
        );

        // Test StockFs basic functionality
        use hypersonic::Stock;
        use ultrasonic::Opid;

        let test_opid = Opid::from([1u8; 32]);
        assert!(
            !stock.has_operation(test_opid),
            "New StockFs should not have test operation"
        );

        // The integration chain is complete: Articles → EffectiveState → Operation → StockFs
        // All components are properly connected and functional
    }

    #[test]
    fn test_sandbox_stock_delta_over_base() {
        use hypersonic::Stock;
        use rgb_delta_store::rgb_components;
        use rgb_delta_store::{SandboxConfig, SandboxStock};
        use ultrasonic::Opid;

        // Create RGB components (no external dependencies)
        let articles =
            rgb_components::create_rgb20_articles().expect("Should be able to create Articles");
        let effective_state = rgb_components::create_effective_state(&articles)
            .expect("Should be able to create EffectiveState");

        // Create sandbox configuration
        let config = SandboxConfig::temp().expect("Should be able to create sandbox config");

        // Create SandboxStock - first create base, then sandbox
        let base_temp = tempfile::tempdir().expect("Failed to create temp dir for base");
        let base = rgb_delta_store::rgb_components::create_test_stock(
            articles.clone(),
            effective_state,
            base_temp.path().to_path_buf(),
        )
        .expect("Should be able to create base stock");
        let mut sandbox = SandboxStock::create_with_base(config.clone(), base)
            .expect("Should be able to create SandboxStock");

        // Verify initial state
        assert_eq!(
            sandbox.operation_count(),
            0,
            "New sandbox should start with 0 operations"
        );
        assert_ne!(
            config.base_path, config.delta_path,
            "Sandbox should use separate base and delta paths"
        );

        // Create and add test operation
        let operation = rgb_components::create_test_operation(&articles, None)
            .expect("Should be able to create test operation");

        let test_opid = Opid::from([1u8; 32]);

        // Verify operation doesn't exist initially
        assert!(
            !sandbox.has_operation(test_opid),
            "New sandbox should not have test operation"
        );

        // Add operation to sandbox
        sandbox.add_operation(test_opid, &operation);

        // Verify operation was added
        assert!(
            sandbox.has_operation(test_opid),
            "Operation should be present after adding"
        );
        assert!(
            sandbox.operation_count() > 0,
            "Operation count should increase after adding operation"
        );

        // Verify operation data integrity
        let retrieved_op = sandbox.operation(test_opid);
        assert_eq!(
            retrieved_op.contract_id, operation.contract_id,
            "Retrieved operation should match original"
        );
        assert_eq!(
            retrieved_op.call_id, operation.call_id,
            "Retrieved operation CallId should match original"
        );

        // Test commit transaction
        sandbox.commit_transaction(); // Should not panic or error

        // Verify operation is still present after commit
        assert!(
            sandbox.has_operation(test_opid),
            "Operation should still be present after commit"
        );

        // Test that operations appear in iterator
        let mut found_in_iterator = false;
        for (iter_opid, iter_op) in sandbox.operations() {
            if iter_opid == test_opid {
                assert_eq!(
                    iter_op.contract_id, operation.contract_id,
                    "Operation from iterator should match original"
                );
                found_in_iterator = true;
                break;
            }
        }
        assert!(
            found_in_iterator,
            "Added operation should appear in operations iterator"
        );
    }

    #[test]
    fn test_sandbox_rollback_logical_isolation() {
        use hypersonic::Stock;
        use rgb_delta_store::rgb_components;
        use rgb_delta_store::{SandboxConfig, SandboxStock};
        use ultrasonic::Opid;

        // Create RGB components (no external dependencies)
        let articles =
            rgb_components::create_rgb20_articles().expect("Should be able to create Articles");
        let effective_state = rgb_components::create_effective_state(&articles)
            .expect("Should be able to create EffectiveState");

        // Create sandbox configuration with separate directories
        let config = SandboxConfig::temp().expect("Should be able to create sandbox config");

        // Create SandboxStock - first create base, then sandbox
        let base_temp = tempfile::tempdir().expect("Failed to create temp dir for base");
        let base = rgb_components::create_test_stock(
            articles.clone(),
            effective_state,
            base_temp.path().to_path_buf(),
        )
        .expect("Should be able to create base stock");
        let mut sandbox = SandboxStock::create_with_base(config, base)
            .expect("Should be able to create SandboxStock");

        // Create test operations
        let operation1 = rgb_components::create_test_operation(&articles, None)
            .expect("Should be able to create test operation 1");
        let operation2 = rgb_components::create_test_operation(&articles, Some([2u8; 32].into()))
            .expect("Should be able to create test operation 2");

        let opid1 = Opid::from([1u8; 32]);
        let opid2 = Opid::from([2u8; 32]);

        // Phase 1: Normal operation visibility
        sandbox.add_operation(opid1, &operation1);
        sandbox.add_operation(opid2, &operation2);

        // Verify visibility
        assert!(
            sandbox.has_operation(opid1),
            "Operation 1 should be visible in normal state"
        );
        assert!(
            sandbox.has_operation(opid2),
            "Operation 2 should be visible in normal state"
        );

        let initial_count = sandbox.operation_count();
        assert_eq!(
            initial_count, 2,
            "Should have 2 operations after adding both"
        );

        // Verify operations iterator includes both
        let mut ops_found = 0;
        for (iter_opid, _) in sandbox.operations() {
            if iter_opid == opid1 || iter_opid == opid2 {
                ops_found += 1;
            }
        }
        assert_eq!(ops_found, 2, "Both operations should be found in iterator");

        // Phase 2: Rollback - logical isolation
        sandbox.rollback().expect("Rollback should succeed");
        assert!(
            sandbox.is_rolled_back(),
            "Sandbox should be marked as rolled back"
        );

        // After rollback, delta operations should be invisible
        assert!(
            !sandbox.has_operation(opid1),
            "Operation 1 should be invisible after rollback"
        );
        assert!(
            !sandbox.has_operation(opid2),
            "Operation 2 should be invisible after rollback"
        );

        // Verify operations iterator no longer includes delta operations
        let mut ops_found_after_rollback = 0;
        for (iter_opid, _) in sandbox.operations() {
            if iter_opid == opid1 || iter_opid == opid2 {
                ops_found_after_rollback += 1;
            }
        }
        assert_eq!(
            ops_found_after_rollback, 0,
            "No delta operations should be found after rollback"
        );

        // Phase 3: Resume operations - visibility restored
        sandbox
            .resume_delta_operations()
            .expect("Resume should succeed");
        assert!(
            !sandbox.is_rolled_back(),
            "Sandbox should no longer be marked as rolled back"
        );

        // After resume, delta operations should be visible again
        assert!(
            sandbox.has_operation(opid1),
            "Operation 1 should be visible again after resume"
        );
        assert!(
            sandbox.has_operation(opid2),
            "Operation 2 should be visible again after resume"
        );

        // Verify operations iterator includes both again
        let mut ops_found_after_resume = 0;
        for (iter_opid, _) in sandbox.operations() {
            if iter_opid == opid1 || iter_opid == opid2 {
                ops_found_after_resume += 1;
            }
        }
        assert_eq!(
            ops_found_after_resume, 2,
            "Both operations should be found again after resume"
        );

        // Phase 4: Verify data integrity
        let retrieved_op1 = sandbox.operation(opid1);
        let retrieved_op2 = sandbox.operation(opid2);

        assert_eq!(
            retrieved_op1.contract_id, operation1.contract_id,
            "Operation 1 data should be intact"
        );
        assert_eq!(
            retrieved_op2.contract_id, operation2.contract_id,
            "Operation 2 data should be intact"
        );

        // Final verification: commit transaction works
        sandbox.commit_transaction(); // Should not panic
        assert!(
            sandbox.has_operation(opid1),
            "Operation 1 should still be present after commit"
        );
        assert!(
            sandbox.has_operation(opid2),
            "Operation 2 should still be present after commit"
        );
    }
}
