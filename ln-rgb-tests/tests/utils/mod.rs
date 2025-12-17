// Test utilities for Lightning RGB tests
//
// This module provides async test utilities adapted from rgb-tests

pub mod asset_params;
pub mod chain;
pub mod multisig;
pub mod test_helpers;
pub mod wallet;

// Constants
pub const INSTANCE_1: u8 = 1;
pub const INSTANCE_2: u8 = 2;
pub const INSTANCE_3: u8 = 3;

// Only Esplora for regtest (Lightning RGB wallet uses Esplora)
pub const ESPLORA_1_REGTEST_URL: &str = "http://127.0.0.1:3001";
pub const ESPLORA_2_REGTEST_URL: &str = "http://127.0.0.1:3002";
pub const ESPLORA_3_REGTEST_URL: &str = "http://127.0.0.1:3003";

// Re-export commonly used types
pub use asset_params::*;
pub use chain::initialize;
pub use test_helpers::*;
pub use wallet::*;

// Standard library imports
pub use std::{
    collections::{BTreeMap, HashMap},
    env::VarError,
    fmt::{self, Display},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::{Mutex, Once, OnceLock, RwLock},
    time::Duration,
};

// External crate imports
pub use amplify::{s, ByteArray};
pub use bp::{Outpoint, Sats, Txid};
pub use bpstd::Network;
pub use esplora::blocking::BlockingClient as EsploraClient;
pub use once_cell::sync::Lazy;
pub use time::OffsetDateTime;

// Type conversion utilities for downstream usage
// Due to Rust orphan rules, we can't implement From<bpstd::Network> for bitcoin::Network
// So we provide a helper function instead

/// Convert bpstd::Network to bitcoin::Network
pub fn to_bitcoin_network(network: bpstd::Network) -> bitcoin::Network {
    match network {
        bpstd::Network::Mainnet => bitcoin::Network::Bitcoin,
        bpstd::Network::Testnet3 => bitcoin::Network::Testnet,
        bpstd::Network::Regtest => bitcoin::Network::Regtest,
        bpstd::Network::Signet => bitcoin::Network::Signet,
        _ => bitcoin::Network::Regtest,
    }
}
