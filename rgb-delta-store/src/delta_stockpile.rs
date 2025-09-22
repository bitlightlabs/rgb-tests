// RGB Delta Stockpile Implementation
//
// This implements a delta-over-base version of the Stockpile trait that uses a
// two-layer approach: a read-only base layer and a writable delta layer.
//
// Key principles:
// - Read operations check delta first, then fall back to base
// - Write operations only affect the delta layer
// - Uses SandboxStock and SandboxPile for contract storage
// - Maintains contract and issuer metadata in both layers

use std::collections::HashMap;
use std::convert::Infallible;
use std::ffi::OsStr;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fs, io};

use crate::{SandboxConfig, SandboxError, SandboxPile, SandboxStock};
use amplify::MultiError;
use rgb::{
    Articles, CodexId, Consensus, Consignment, ConsumeError, Contract, ContractId, CreateParams,
    Issuer, IssuerError, Pile, RgbSeal, Stock, Stockpile,
};

/// A trait for stockpile implementations that support a transactional,
/// delta-over-base layer model.
///
/// This extends the base `Stockpile` trait with methods for committing
/// and rolling back state changes, providing atomic operations over a
/// set of contracts.
pub trait DeltaStockpile: Stockpile {
    /// The type of the read-only view of the base layer.
    /// This view itself must also implement `Stockpile` to be useful.
    type BaseView<'a>: Stockpile<Stock = Self::Stock, Pile = Self::Pile, Error = Self::Error>
    where
        Self: 'a;

    /// Returns a read-only view of the base layer, ignoring any
    /// changes made in the current delta.
    ///
    /// This is useful for comparing the state before and after a series
    /// of operations within a transaction.
    fn base(&self) -> Self::BaseView<'_>;

    /// Commits all pending changes from the delta layer to the
    /// base layer, making them permanent.
    ///
    /// After a successful commit, the delta layer is cleared, and the
    /// base layer reflects the new state.
    fn commit(&mut self) -> Result<(), Self::Error>;

    /// Discards all pending changes in the delta layer, reverting
    /// to the last committed state of the base layer.
    fn revert(&mut self) -> Result<(), Self::Error>;
}

/// A delta-over-base implementation of Stockpile focused on state management
/// for Lightning Network RGB transactions. The delta layer only manages state changes
/// of existing contracts from the base layer, without creating new contracts or issuers.
#[derive(Clone, Debug)]
pub struct DeltaStockpileDir<Seal: RgbSeal> {
    consensus: Consensus,
    testnet: bool,
    /// Base directory for read-only stockpile data (contains all contracts/issuers)
    base_dir: PathBuf,
    /// Delta directory for state modifications only  
    delta_dir: PathBuf,
    /// Configuration for sandbox operations
    config: SandboxConfig,
    /// Cached issuer metadata from base layer (immutable during delta operations)
    base_issuers: HashMap<CodexId, String>,
    /// Cached contract metadata from base layer (immutable during delta operations)
    base_contracts: HashMap<ContractId, String>,
    _phantom: PhantomData<Seal>,
}

impl<Seal: RgbSeal> DeltaStockpileDir<Seal>
where
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>,
{
    /// Create a new delta stockpile with existing base directory and new delta directory
    pub fn load(
        base_dir: PathBuf,
        delta_dir: PathBuf,
        consensus: Consensus,
        testnet: bool,
    ) -> Result<Self, io::Error> {
        // Load base layer metadata
        let mut base_issuers = HashMap::new();
        let mut base_contracts = HashMap::new();

        if base_dir.exists() {
            let readdir = fs::read_dir(&base_dir)?;
            for entry in readdir {
                let entry = entry?;
                let path = entry.path();
                let ty = entry.file_type()?;
                let Some(extension) = path.extension().and_then(OsStr::to_str) else {
                    continue;
                };
                let Some(name) = path.file_stem().and_then(OsStr::to_str) else {
                    continue;
                };
                let Some((name, id_str)) = name.split_once('.') else {
                    continue;
                };
                if ty.is_file() && extension == "issuer" {
                    let Ok(id) = CodexId::from_str(id_str) else {
                        continue;
                    };
                    base_issuers.insert(id, name.to_string());
                } else if ty.is_dir() && extension == "contract" {
                    let Ok(id) = ContractId::from_str(id_str) else {
                        continue;
                    };
                    base_contracts.insert(id, name.to_string());
                }
            }
        }

        // Create delta directory if it doesn't exist (for state modifications only)
        if !delta_dir.exists() {
            fs::create_dir_all(&delta_dir)?;
        }

        let config = SandboxConfig::new(base_dir.clone(), delta_dir.clone());

        Ok(Self {
            consensus,
            testnet,
            base_dir,
            delta_dir,
            config,
            base_issuers,
            base_contracts,
            _phantom: PhantomData,
        })
    }

    /// Get the base directory path
    pub fn base_dir(&self) -> &Path {
        self.base_dir.as_path()
    }

    /// Get the delta directory path  
    pub fn delta_dir(&self) -> &Path {
        self.delta_dir.as_path()
    }

    /// Get sandbox configuration
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Get contract directory path
    fn get_contract_dir(&self, contract_id: ContractId) -> Option<PathBuf> {
        if let Some(subdir) = self.base_contracts.get(&contract_id) {
            let path = self
                .base_dir
                .join(format!("{subdir}.{contract_id:-}.contract"));
            return Some(path);
        }

        None
    }

    /// Get contract dir as SandboxConfig
    fn get_contract_config(&self, contract_id: ContractId) -> Option<SandboxConfig> {
        let sp_cfg = self.config.clone();
        if let Some(subdir) = self.base_contracts.get(&contract_id) {
            let path_suffix = format!("{subdir}.{contract_id:-}.contract");
            Some(SandboxConfig {
                base_path: sp_cfg.base_path.join(path_suffix.clone()),
                delta_path: sp_cfg.delta_path.join(path_suffix.clone()),
            })
        } else {
            None
        }
    }

    /// Create a new contract directory in base layer
    fn create_contract_dir(&mut self, articles: &Articles) -> io::Result<SandboxConfig> {
        let contract_id = articles.contract_id();
        let name = articles.issue().meta.name.clone();
        let subdir = format!("{}.{contract_id:-}.contract", name);
        let path = self.base_dir.join(&subdir);
        let delta_path = self.delta_dir.join(&subdir);

        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        // Add to base contracts metadata
        self.base_contracts.insert(contract_id, name.to_string());

        Ok(SandboxConfig {
            base_path: path.clone(),
            delta_path: delta_path.clone(),
        })
    }


    /// Get a view of only the base layer (without delta changes)
    pub fn base(&self) -> BaseStockpileView<'_, Seal> {
        BaseStockpileView {
            consensus: self.consensus,
            testnet: self.testnet,
            base_dir: &self.base_dir,
            base_issuers: &self.base_issuers,
            base_contracts: &self.base_contracts,
            _phantom: PhantomData,
        }
    }
}

/// A view that only exposes base layer data (no delta)
#[derive(Debug)]
pub struct BaseStockpileView<'a, Seal: RgbSeal> {
    consensus: Consensus,
    testnet: bool,
    base_dir: &'a PathBuf,
    base_issuers: &'a HashMap<CodexId, String>,
    base_contracts: &'a HashMap<ContractId, String>,
    _phantom: PhantomData<Seal>,
}

impl<'a, Seal: RgbSeal> BaseStockpileView<'a, Seal> {
    pub fn consensus(&self) -> Consensus {
        self.consensus
    }

    pub fn is_testnet(&self) -> bool {
        self.testnet
    }

    pub fn issuers_count(&self) -> usize {
        self.base_issuers.len()
    }

    pub fn contracts_count(&self) -> usize {
        self.base_contracts.len()
    }

    pub fn has_issuer(&self, codex_id: CodexId) -> bool {
        self.base_issuers.contains_key(&codex_id)
    }

    pub fn has_contract(&self, contract_id: ContractId) -> bool {
        self.base_contracts.contains_key(&contract_id)
    }

    pub fn codex_ids(&self) -> impl Iterator<Item = CodexId> + '_ {
        self.base_issuers.keys().copied()
    }

    pub fn contract_ids(&self) -> impl Iterator<Item = ContractId> + '_ {
        self.base_contracts.keys().copied()
    }
}

impl<'a, Seal: RgbSeal> Stockpile for BaseStockpileView<'a, Seal>
where
    Seal::Client: strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::Published: Eq + strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>,
{
    type Stock = SandboxStock;
    type Pile = SandboxPile<Seal>;
    type Error = io::Error;

    fn consensus(&self) -> Consensus {
        self.consensus
    }

    fn is_testnet(&self) -> bool {
        self.testnet
    }

    fn issuers_count(&self) -> usize {
        self.base_issuers.len()
    }

    fn contracts_count(&self) -> usize {
        self.base_contracts.len()
    }

    fn has_issuer(&self, codex_id: CodexId) -> bool {
        self.base_issuers.contains_key(&codex_id)
    }

    fn has_contract(&self, contract_id: ContractId) -> bool {
        self.base_contracts.contains_key(&contract_id)
    }

    fn codex_ids(&self) -> impl Iterator<Item = CodexId> {
        self.base_issuers.keys().copied()
    }

    fn contract_ids(&self) -> impl Iterator<Item = ContractId> {
        self.base_contracts.keys().copied()
    }

    fn issuer(&self, codex_id: CodexId) -> Option<Issuer> {
        let name = self.base_issuers.get(&codex_id)?;
        let path = self.base_dir.join(format!("{name}.{codex_id:#}.issuer"));
        Issuer::load(path, |_, _, _| -> Result<_, Infallible> { Ok(()) }).ok()
    }

    fn contract(&self, contract_id: ContractId) -> Option<Contract<Self::Stock, Self::Pile>> {
        let name = self.base_contracts.get(&contract_id)?;
        let subdir = format!("{name}.{contract_id:-}.contract");
        let path = self.base_dir.join(&subdir);
        if !path.exists() {
            return None;
        }

        // Create a temporary config for this contract
        let config = SandboxConfig {
            base_path: path.clone(),
            delta_path: path.clone(), // For base view, delta path same as base
        };

        let contract = Contract::load(config.clone(), config.clone()).ok()?;
        let meta = &contract.articles().issue().meta;
        if meta.consensus != self.consensus || meta.testnet != self.testnet {
            return None;
        }
        Some(contract)
    }

    fn import_issuer(&mut self, _issuer: Issuer) -> Result<Issuer, Self::Error> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Cannot import issuers in base view",
        ))
    }

    fn import_contract(
        &mut self,
        _articles: Articles,
        _consignment: Consignment<Seal>,
    ) -> Result<
        Contract<Self::Stock, Self::Pile>,
        MultiError<
            ConsumeError<Seal::Definition>,
            <Self::Stock as Stock>::Error,
            <Self::Pile as Pile>::Error,
        >,
    >
    where
        Seal::Client: strict_encoding::StrictDecode,
        Seal::Published: strict_encoding::StrictDecode,
        Seal::WitnessId: strict_encoding::StrictDecode,
    {
        Err(MultiError::C(SandboxError::DataNotFound))
    }

    fn issue(
        &mut self,
        _params: CreateParams<<<Self::Pile as Pile>::Seal as RgbSeal>::Definition>,
    ) -> Result<
        Contract<Self::Stock, Self::Pile>,
        MultiError<IssuerError, SandboxError, SandboxError>,
    > {
        Err(MultiError::B(SandboxError::DataNotFound))
    }

    fn purge(&mut self, _contract_id: ContractId) -> Result<(), Self::Error> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Cannot purge contracts in base view",
        ))
    }
}

impl<Seal: RgbSeal> Stockpile for DeltaStockpileDir<Seal>
where
    Seal::Client: strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::Published: Eq + strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>,
{
    type Stock = SandboxStock;
    type Pile = SandboxPile<Seal>;
    type Error = io::Error;

    fn consensus(&self) -> Consensus {
        self.consensus
    }

    fn is_testnet(&self) -> bool {
        self.testnet
    }

    fn issuers_count(&self) -> usize {
        self.base_issuers.len()
    }

    fn contracts_count(&self) -> usize {
        self.base_contracts.len()
    }

    fn has_issuer(&self, codex_id: CodexId) -> bool {
        self.base_issuers.contains_key(&codex_id)
    }

    fn has_contract(&self, contract_id: ContractId) -> bool {
        self.base_contracts.contains_key(&contract_id)
    }

    fn codex_ids(&self) -> impl Iterator<Item = CodexId> {
        self.base_issuers.keys().copied()
    }

    fn contract_ids(&self) -> impl Iterator<Item = ContractId> {
        self.base_contracts.keys().copied()
    }

    fn issuer(&self, codex_id: CodexId) -> Option<Issuer> {
        let name = self.base_issuers.get(&codex_id)?;
        let path = self.base_dir.join(format!("{name}.{codex_id:#}.issuer"));
        Issuer::load(path, |_, _, _| -> Result<_, Infallible> { Ok(()) }).ok()
    }

    fn contract(&self, contract_id: ContractId) -> Option<Contract<Self::Stock, Self::Pile>> {
        let path = self.get_contract_dir(contract_id)?;
        let contract = Contract::load(self.config.clone(), self.config.clone()).ok()?;
        let meta = &contract.articles().issue().meta;
        if meta.consensus != self.consensus || meta.testnet != self.testnet {
            return None;
        }
        Some(contract)
    }

    fn import_issuer(&mut self, issuer: Issuer) -> Result<Issuer, Self::Error> {
        let codex_id = issuer.codex_id();
        let name = issuer.codex().name.to_string();
        let path = self.base_dir.join(format!("{name}.{codex_id:#}.issuer"));
        issuer.save(path)?;
        self.base_issuers.insert(codex_id, name);
        Ok(issuer)
    }

    fn import_contract(
        &mut self,
        articles: Articles,
        consignment: Consignment<Seal>,
    ) -> Result<
        Contract<Self::Stock, Self::Pile>,
        MultiError<
            ConsumeError<Seal::Definition>,
            <Self::Stock as Stock>::Error,
            <Self::Pile as Pile>::Error,
        >,
    >
    where
        Seal::Client: strict_encoding::StrictDecode,
        Seal::Published: strict_encoding::StrictDecode,
        Seal::WitnessId: strict_encoding::StrictDecode,
    {
        let dir = self
            .create_contract_dir(&articles)
            .map_err(|io_error| MultiError::C(io_error.into()))?;
        let contract = Contract::with(articles, consignment, dir)?;
        self.base_contracts.insert(
            contract.contract_id(),
            contract.articles().issue().meta.name.to_string(),
        );
        Ok(contract)
    }

    fn issue(
        &mut self,
        params: CreateParams<<<Self::Pile as Pile>::Seal as RgbSeal>::Definition>,
    ) -> Result<
        Contract<Self::Stock, Self::Pile>,
        MultiError<IssuerError, SandboxError, SandboxError>,
    > {
        let schema = self.issuer(params.issuer.codex_id()).ok_or(MultiError::A(
            IssuerError::UnknownCodex(params.issuer.codex_id()),
        ))?;
        let contract = Contract::issue(schema, params, |articles| {
            Ok(self.create_contract_dir(articles)?)
        })
        .map_err(MultiError::from_other_a)?;
        self.base_contracts.insert(
            contract.contract_id(),
            contract.articles().issue().meta.name.to_string(),
        );
        Ok(contract)
    }

    fn purge(&mut self, contract_id: ContractId) -> Result<(), Self::Error> {
        let path = self
            .get_contract_dir(contract_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Contract not found"))?;
        fs::remove_dir_all(&path)?;
        self.base_contracts.remove(&contract_id);
        Ok(())
    }
}

impl<Seal: RgbSeal> DeltaStockpile for DeltaStockpileDir<Seal>
where
    Seal::Client: strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::Published: Eq + strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>,
{
    type BaseView<'a> = BaseStockpileView<'a, Seal> where Self: 'a;

    fn base(&self) -> Self::BaseView<'_> {
        BaseStockpileView {
            consensus: self.consensus,
            testnet: self.testnet,
            base_dir: &self.base_dir,
            base_issuers: &self.base_issuers,
            base_contracts: &self.base_contracts,
            _phantom: PhantomData,
        }
    }

    fn commit(&mut self) -> Result<(), Self::Error> {
        // Commit all contracts - let SandboxStock/SandboxPile handle whether they have changes
        for contract_id in self.base_contracts.keys().copied().collect::<Vec<_>>() {
            if let Some(config) = self.get_contract_config(contract_id) {
                // Load and commit stock
                if let Ok(stock) = SandboxStock::load(&config) {
                    stock.commit().map_err(|e| {
                        io::Error::new(io::ErrorKind::Other, format!("Stock commit failed: {}", e))
                    })?;
                }

                // Load and commit pile  
                if let Ok(pile) = SandboxPile::<Seal>::load(&config) {
                    pile.commit().map_err(|e| {
                        io::Error::new(io::ErrorKind::Other, format!("Pile commit failed: {}", e))
                    })?;
                }
            }
        }

        // Clean up delta directory after successful commits
        if self.delta_dir.exists() {
            fs::remove_dir_all(&self.delta_dir)?;
            fs::create_dir_all(&self.delta_dir)?;
        }
        Ok(())
    }

    fn revert(&mut self) -> Result<(), Self::Error> {
        // Revert all contracts - let SandboxStock/SandboxPile handle whether they have changes
        for contract_id in self.base_contracts.keys().copied().collect::<Vec<_>>() {
            if let Some(config) = self.get_contract_config(contract_id) {
                // Load and revert stock
                if let Ok(stock) = SandboxStock::load(&config) {
                    let _ = stock.revert();
                }

                // Load and revert pile
                if let Ok(pile) = SandboxPile::<Seal>::load(&config) {
                    let _ = pile.revert();
                }
            }
        }

        // Clean up delta directory
        if self.delta_dir.exists() {
            fs::remove_dir_all(&self.delta_dir)?;
            fs::create_dir_all(&self.delta_dir)?;
        }
        Ok(())
    }
}
