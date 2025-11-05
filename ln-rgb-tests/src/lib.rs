//! Lightning Network RGB integration tests library
//!
//! Provides common utilities and extensions for Lightning Network RGB testing

pub mod coloring;
pub mod conservation;
pub mod tx_builder;
pub mod wallet_ext;

// Re-export commonly used types
pub use coloring::ColoringHelper;
pub use conservation::ConservationChecker;
pub use tx_builder::TxBuilder;
pub use wallet_ext::WalletExt;

// Re-export test utilities for use in integration tests
#[path = "../tests/utils/mod.rs"]
pub mod utils;
