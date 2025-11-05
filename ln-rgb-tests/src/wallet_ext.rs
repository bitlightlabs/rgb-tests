//! TestWallet Extension Trait
//!
//! Adds Lightning Network-related helper functions to rgb-tests' TestWallet

/// TestWallet Extension Trait
pub trait WalletExt {
    /// Create multi-output PSBT (for simulating Commitment TX)
    fn create_multi_output_psbt(&mut self) -> Result<(), String>;

    /// Extract RGB allocation information from UTXO
    fn extract_rgb_allocations(&self) -> Result<(), String>;

    /// Build ColoringInfo from PSBT
    fn build_coloring_info(&self) -> Result<(), String>;

    /// Verify correctness of colored PSBT
    fn verify_colored_psbt(&self) -> Result<(), String>;
}

// TODO: Implement WalletExt for TestWallet
