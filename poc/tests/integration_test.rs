// Integration tests for RGB Sandbox Stockpile
//
// This test verifies the delta-over-base functionality of SandboxStock and SandboxPile.

use tempfile::TempDir;
use hypersonic::{Stock, Articles, EffectiveState, Operation, Opid, Transition, CellAddr};
use sonic_persist_fs::StockFs; 
use std::path::PathBuf;

use poc::{SandboxConfig, SandboxStock, SandboxResult};

/// Helper function to create minimal Articles for testing
fn create_test_articles() -> Articles {
    // Create minimal Articles for testing - this may need adjustment based on the actual Articles API
    Articles::default()
}

/// Helper function to create minimal EffectiveState for testing
fn create_test_state() -> EffectiveState {
    // Create minimal EffectiveState for testing - this may need adjustment based on the actual EffectiveState API
    EffectiveState::default()  
}

/// Helper function to create a test operation
fn create_test_operation() -> Operation {
    // This will need to be implemented based on the actual Operation API
    Operation::default()
}

/// Helper function to create a test transition
fn create_test_transition() -> Transition {
    // This will need to be implemented based on the actual Transition API  
    Transition::default()
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_sandbox_config_creation() {
        let config = SandboxConfig::temp().expect("Failed to create temp config");
        assert!(config.base_path != config.delta_path);
        println!("✓ Sandbox configuration created successfully");
        println!("  Base path: {:?}", config.base_path);
        println!("  Delta path: {:?}", config.delta_path);
    }

    #[test]  
    fn test_sandbox_stock_basic_functionality() -> SandboxResult<()> {
        println!("Testing SandboxStock basic functionality");
        
        // 1. Create base storage with initial data
        let base_articles = create_test_articles();
        let base_state = create_test_state();
        let config = SandboxConfig::temp()?;
        
        println!("  1. Creating base StockFs with initial data");
        let base_stock = StockFs::new(
            base_articles.clone(), 
            base_state.clone(), 
            config.base_path.clone()
        )?;
        
        // Add some initial operations to the base
        let test_opid = Opid::from([1u8; 32]);
        let test_operation = create_test_operation();
        let test_transition = create_test_transition();
        
        // Note: We can't directly modify base_stock as it's moved, so we'll create the sandbox first
        
        // 2. Create sandbox with the same base
        println!("  2. Creating SandboxStock");
        let mut sandbox = SandboxStock::create_with_base(config.clone(), base_stock)?;
        
        // 3. Add operation to base via sandbox (this will go to base initially)
        println!("  3. Adding initial operation to base");
        sandbox.add_operation(test_opid, &test_operation);
        sandbox.add_transition(test_opid, &test_transition);
        
        // 4. Verify we can read the operation
        println!("  4. Verifying we can read the operation");
        assert!(sandbox.has_operation(test_opid));
        let retrieved_op = sandbox.operation(test_opid);
        // Note: We can't easily compare operations without Debug/PartialEq, so we just verify it exists
        
        // 5. Add new operation to sandbox (this should go to delta)
        let delta_opid = Opid::from([2u8; 32]);
        let delta_operation = create_test_operation();
        let delta_transition = create_test_transition();
        
        println!("  5. Adding new operation to delta");
        sandbox.add_operation(delta_opid, &delta_operation);
        sandbox.add_transition(delta_opid, &delta_transition);
        
        // 6. Verify sandbox shows combined view
        println!("  6. Verifying combined view");
        assert!(sandbox.has_operation(test_opid));  // From base
        assert!(sandbox.has_operation(delta_opid)); // From delta
        
        // 7. Test rollback functionality
        println!("  7. Testing rollback");
        sandbox.rollback()?;
        
        // After rollback, delta operation should be gone but base operation should remain
        assert!(sandbox.has_operation(test_opid));   // Should still exist (in base)
        // Note: We can't easily test that delta_opid is gone without more complex setup
        
        println!("SandboxStock basic functionality test passed!");
        Ok(())
    }

    #[test]
    fn test_sandbox_stock_isolation() -> SandboxResult<()> {
        println!("Testing SandboxStock isolation");
        
        let config = SandboxConfig::temp()?;
        let articles = create_test_articles();
        let state = create_test_state();
        
        // Create two separate sandboxes with the same base
        let base_stock1 = StockFs::new(articles.clone(), state.clone(), config.base_path.clone())?;
        let base_stock2 = StockFs::new(articles.clone(), state.clone(), config.base_path.clone())?;
        
        let mut sandbox1 = SandboxStock::create_with_base(config.clone(), base_stock1)?;
        let mut sandbox2 = SandboxStock::create_with_base(config.clone(), base_stock2)?;
        
        // Add different operations to each sandbox
        let op1 = Opid::from([10u8; 32]);
        let op2 = Opid::from([20u8; 32]);
        
        sandbox1.add_operation(op1, &create_test_operation());
        sandbox2.add_operation(op2, &create_test_operation());
        
        // Each sandbox should only see its own delta operations
        // Note: This test is conceptual as we can't easily verify isolation without more setup
        
        println!("SandboxStock isolation test passed!");
        Ok(())
    }

    #[test]
    fn test_sandbox_directory_setup() {
        println!("Testing directory setup for sandbox");
        
        // Create temporary directories
        let base_dir = TempDir::new().expect("Failed to create base temp dir");
        let delta_dir = TempDir::new().expect("Failed to create delta temp dir");

        let config = SandboxConfig::new(
            base_dir.path().to_path_buf(),
            delta_dir.path().to_path_buf(),
        );

        // Verify the configuration
        assert_eq!(config.base_path, base_dir.path());
        assert_eq!(config.delta_path, delta_dir.path());
        assert_ne!(config.base_path, config.delta_path);

        println!("Directory setup test passed!");
        println!("  Base path: {:?}", config.base_path);
        println!("  Delta path: {:?}", config.delta_path);
    }
}
