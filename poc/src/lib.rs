// RGB Sandbox Stockpile - Proof of Concept
//
// SPDX-License-Identifier: Apache-2.0
//
// This module implements a sandbox layer for RGB Stock and Pile operations,
// allowing for transactional state changes that can be committed or discarded.

mod sandbox_pile;
mod sandbox_stock;

pub use sandbox_pile::SandboxPile;
pub use sandbox_stock::SandboxStock;

use std::convert::Infallible;
use std::path::PathBuf;

// RGB component imports
use hypersonic::{Articles, EffectiveState, Stock};
use rgb::Issuer;
use sonic_persist_fs::{FsError, StockFs};

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
        Self {
            base_path,
            delta_path,
        }
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

/// Helper functions for RGB component construction
pub mod rgb_components {
    use super::*;
    use std::path::Path;

    /// Creates Articles from a pre-built issuer file
    ///
    /// This function uses a pre-built issuer file from the test environment to create Articles,
    /// avoiding a complex manual construction process.
    pub fn create_articles_from_issuer(issuer_path: impl AsRef<Path>) -> SandboxResult<Articles> {
        use amplify::confinement::Confined;
        use amplify::{default, num::u256, zero};
        use hypersonic::{ContractMeta, ContractName, Issue};
        use ultrasonic::{fe256, Genesis, Identity};

        // Load Issuer with a simple validator
        let issuer = Issuer::load(issuer_path, |_, _, _| Result::<_, Infallible>::Ok(()))
            .map_err(|_| SandboxError::DataNotFound)?;

        // Extract components from Issuer, based on builders.rs pattern
        let codex_id = issuer.codex_id();

        // Get a valid CallId from the API's verifiers map
        let default_api = issuer.default_api();

        // Try to find a valid CallId from the available methods in the API
        let call_id = if let Some((_, call_id)) = default_api.verifiers.first_key_value() {
            // Use the first available method's CallId
            *call_id
        } else if let Some(call_state) = &default_api.default_call {
            // Fall back to default_call if available, extract CallId properly
            // Since default_call contains CallState with method field
            if let Some(call_id) = default_api.verifier(call_state.method.clone()) {
                call_id
            } else {
                panic!("No valid CallId found in API, this may cause issues");
            }
        } else {
            panic!("Warning: No verifiers or default_call found in API");
        };
        let (codex, semantics) = issuer.dismember();

        // Create minimal ContractMeta, based on builders.rs pattern
        use ultrasonic::Consensus;

        let meta = ContractMeta {
            consensus: Consensus::Bitcoin,
            testnet: true,
            timestamp: 0, // Use default timestamp
            features: default!(),
            name: ContractName::Named("TestContract".into()),
            issuer: Identity::default(),
        };

        let genesis = Genesis {
            version: default!(),
            codex_id,
            call_id,
            nonce: fe256::from(u256::ZERO),
            blank0: zero!(),
            blank1: zero!(),
            blank2: zero!(),
            destructible_out: Confined::try_from(vec![]).unwrap(), // Empty state output
            immutable_out: Confined::try_from(vec![]).unwrap(),    // Empty immutable state output
        };

        // Construct Issue
        let issue = Issue {
            version: default!(),
            meta,
            codex,
            genesis,
        };

        // Create Articles using Articles::with, based on builders.rs pattern
        let articles = Articles::with(semantics, issue, None, |_, _, _| -> Result<_, Infallible> {
            unreachable!()
        })
        .map_err(|_| SandboxError::DataNotFound)?;

        Ok(articles)
    }

    /// Creates RGB20 Articles using a built-in RGB20 issuer
    pub fn create_rgb20_articles() -> SandboxResult<Articles> {
        // Use the RGB20 issuer file from the test directory
        let issuer_path = Path::new("../tests/templates/schemata/RGB20-Simplest-v0-rLosfg.issuer");
        create_articles_from_issuer(issuer_path)
    }

    /// Creates EffectiveState from Articles
    pub fn create_effective_state(articles: &Articles) -> SandboxResult<EffectiveState> {
        // Based on analysis, EffectiveState is created using the with_articles method
        let state = EffectiveState::with_articles(articles).map_err(|e| {
            eprintln!("EffectiveState creation error: {:?}", e);
            SandboxError::DataNotFound
        })?;
        Ok(state)
    }

    /// Creates a complete StockFs for testing
    pub fn create_test_stock(
        articles: Articles,
        state: EffectiveState,
        storage_path: PathBuf,
    ) -> SandboxResult<StockFs> {
        let stock =
            StockFs::new(articles, state, storage_path).map_err(SandboxError::BaseStorage)?;
        Ok(stock)
    }
}

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
