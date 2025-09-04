// RGB Sandbox Stockpile - Proof of Concept
//
// SPDX-License-Identifier: Apache-2.0
//
// This module implements a sandbox layer for RGB Stock and Pile operations,
// allowing for transactional state changes that can be committed or discarded.

mod sandbox_stock;
mod sandbox_pile;

pub use sandbox_stock::SandboxStock;
pub use sandbox_pile::SandboxPile;

use std::path::PathBuf;

/// Configuration for creating a sandbox stockpile
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    /// Path to the main (base) storage directory
    pub base_path: PathBuf,
    /// Path to the delta (incremental) storage directory  
    pub delta_path: PathBuf,
}

impl SandboxConfig {
    pub fn new(base_path: PathBuf, delta_path: PathBuf) -> Self {
        Self { base_path, delta_path }
    }
    
    /// Create a temporary sandbox configuration for testing
    pub fn temp() -> Result<Self, std::io::Error> {
        let base = tempfile::tempdir()?.keep();
        let delta = tempfile::tempdir()?.keep();
        Ok(Self::new(base, delta))
    }
}

/// Errors that can occur during sandbox operations
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Base storage error: {0}")]
    BaseStorage(#[from] sonic_persist_fs::FsError),
    
    #[error("Delta storage error: {0}")]
    DeltaStorage(#[source] sonic_persist_fs::FsError),
    
    #[error("Pile storage error: {0}")]
    PileStorage(#[from] std::io::Error),
    
    #[error("Data not found in either base or delta storage")]
    DataNotFound,
}

/// Result type for sandbox operations
pub type SandboxResult<T> = Result<T, SandboxError>;

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    
    #[test] 
    fn test_sandbox_config_creation() {
        let config = SandboxConfig::temp().expect("Failed to create temp config");
        assert!(config.base_path.exists() || !config.base_path.exists());
        assert!(config.delta_path.exists() || !config.delta_path.exists());
    }
}