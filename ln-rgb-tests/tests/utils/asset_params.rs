// Asset parameter builders for Lightning RGB tests

use bp::Outpoint;
use hypersonic::{CodexId, Consensus, IssuerSpec, NamedState};
use rgb::{Assignment, CreateParams, EitherSeal};
use std::fs::File;
use std::path::Path;
use std::str::FromStr;
use strict_encoding::{TypeName, VariantName};
use strict_types::StrictVal;

// Template paths
const NON_INFLATABLE_ASSET_TEMPLATE_PATH: &str = "tests/templates/issuance/NFA.yaml";
const SCHEMATA_DIR: &str = "tests/templates/schemata";

/// Parameters for NIA (Non-Inflatable Asset) issuance
#[derive(Clone)]
pub struct NIAIssueParams {
    pub name: String,
    pub ticker: String,
    pub precision: String,
    pub circulating_supply: u64,
    pub initial_allocations: Vec<(Outpoint, u64)>,
}

impl Default for NIAIssueParams {
    fn default() -> Self {
        Self {
            name: "USD Tether".to_string(),
            ticker: "USDT".to_string(),
            precision: "centiMilli".to_string(),
            circulating_supply: 1_000_000,
            initial_allocations: vec![],
        }
    }
}

impl NIAIssueParams {
    pub fn new(
        name: impl Into<String>,
        ticker: impl Into<String>,
        precision: impl Into<String>,
        circulating_supply: u64,
    ) -> Self {
        Self {
            name: name.into(),
            ticker: ticker.into(),
            precision: precision.into(),
            circulating_supply,
            initial_allocations: vec![],
        }
    }

    pub fn add_allocation(&mut self, outpoint: Outpoint, amount: u64) -> &mut Self {
        self.initial_allocations.push((outpoint, amount));
        self
    }

    /// Convert NIAIssueParams to CreateParams using AssetParamsBuilder
    pub fn into_create_params(self) -> CreateParams<Outpoint> {
        let mut builder: AssetParamsBuilder = AssetParamsBuilder::default_nia()
            .name(self.name.as_str())
            .update_name_state(self.name.as_str())
            .update_ticker_state(self.ticker.as_str())
            .update_precision_state(self.precision.as_str())
            .update_circulating_state(self.circulating_supply)
            .clear_owned_state();

        for (outpoint, amount) in self.initial_allocations {
            builder = builder.add_owned_state(outpoint, amount);
        }

        builder.build()
    }
}

/// RGB Contract State representation
#[derive(Debug)]
pub struct ContractState {
    /// Immutable state of the contract
    pub immutable: ContractImmutableState,
    /// Ownership state of the contract
    pub owned: ContractOwnedState,
}

/// Contract's immutable state
#[derive(Debug)]
pub struct ContractImmutableState {
    pub name: String,
    pub ticker: String,
    pub precision: String,
    pub circulating_supply: u64,
}

/// Contract's ownership state
/// Simplified version using String for seal representation
#[derive(Debug)]
pub struct ContractOwnedState {
    pub allocations: Vec<(String, u64)>,
}

/// RGB Asset creation parameters builder
#[derive(Clone)]
pub struct AssetParamsBuilder {
    params: CreateParams<Outpoint>,
}

impl AssetParamsBuilder {
    /// Create a new builder instance for non-inflatable asset
    pub fn default_nia() -> Self {
        Self {
            params: Self::from_file(NON_INFLATABLE_ASSET_TEMPLATE_PATH),
        }
    }

    /// Load parameters from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> CreateParams<Outpoint> {
        let file = File::open(path).expect("Unable to open file");
        let params: CreateParams<Outpoint> =
            serde_yaml::from_reader::<_, CreateParams<Outpoint>>(file).expect("");
        params
    }

    /// Set the contract template ID
    pub fn codex_id(mut self, codex_id: CodexId) -> Self {
        self.params.issuer = IssuerSpec::Latest(codex_id);
        self
    }

    /// Set the consensus type
    pub fn consensus(mut self, consensus: Consensus) -> Self {
        self.params.consensus = consensus;
        self
    }

    /// Set whether it is a test network
    pub fn testnet(mut self, testnet: bool) -> Self {
        self.params.testnet = testnet;
        self
    }

    /// Set the contract method name
    pub fn method(mut self, method: &str) -> Self {
        self.params.method = VariantName::from_str(method).unwrap();
        self
    }

    /// Set the contract name
    pub fn name(mut self, name: &str) -> Self {
        self.params.name = TypeName::from_str(name).unwrap();
        self
    }

    /// Update name state in global states
    pub fn update_name_state(mut self, value: &str) -> Self {
        if let Some(state) = self
            .params
            .global
            .iter_mut()
            .find(|s| s.name == "name".into())
        {
            state.state.verified = value.into();
        }
        self
    }

    /// Update details state in global states
    pub fn update_details_state(mut self, value: &str) -> Self {
        if let Some(state) = self
            .params
            .global
            .iter_mut()
            .find(|s| s.name == "details".into())
        {
            state.state.verified = StrictVal::Unit;
            state.state.unverified = Some(value.into());
        }
        self
    }

    /// Update ticker state in global states
    pub fn update_ticker_state(mut self, value: &str) -> Self {
        if let Some(state) = self
            .params
            .global
            .iter_mut()
            .find(|s| s.name == "ticker".into())
        {
            state.state.verified = value.into();
        }
        self
    }

    /// Update precision state in global states
    pub fn update_precision_state(mut self, value: &str) -> Self {
        if let Some(state) = self
            .params
            .global
            .iter_mut()
            .find(|s| s.name == "precision".into())
        {
            state.state.verified = value.into();
        }
        self
    }

    /// Update circulating state in global states
    /// circulating type is "RGBContract.Amount" eq u64 in rust
    pub fn update_circulating_state(mut self, value: u64) -> Self {
        if let Some(state) = self
            .params
            .global
            .iter_mut()
            .find(|s| s.name == "issued".into())
        {
            state.state.verified = value.into();
        }
        self
    }

    /// Update owned state
    pub fn update_owned_state(mut self, seal: Outpoint, val: u64) -> Self {
        // check if owned state exists
        if let Some(state) = self
            .params
            .owned
            .iter_mut()
            .find(|s| s.name == "owned".into())
        {
            // if exists, update seal and data
            state.state.seal = EitherSeal::Alt(seal);
            state.state.data = val.into();
        } else {
            // if not exists, create a new owned state
            self.params.owned.push(NamedState {
                name: "owned".into(),
                state: Assignment {
                    seal: EitherSeal::Alt(seal),
                    data: val.into(),
                },
            });
        }
        self
    }

    pub fn clear_owned_state(mut self) -> Self {
        self.params.owned.clear();
        self
    }

    /// Add owned state
    pub fn add_owned_state(mut self, seal: Outpoint, val: u64) -> Self {
        self.params.owned.push(NamedState {
            name: "balance".into(),
            state: Assignment {
                seal: EitherSeal::Alt(seal),
                data: val.into(),
            },
        });
        self
    }

    /// Build CreateParams instance
    pub fn build(self) -> CreateParams<Outpoint> {
        self.params
    }
}
