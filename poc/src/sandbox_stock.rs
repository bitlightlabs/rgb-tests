// RGB Sandbox Stock Implementation
//
// This implements a sandboxed version of the Stock trait that uses a delta-over-base
// storage pattern. Read operations check the delta storage first, then fall back to
// the base storage. Write operations only affect the delta storage.

use amplify::MultiError;
use hypersonic::{
    Articles, EffectiveState, Operation, Opid, Transition, CellAddr, 
    Stock, SemanticError
};
use sonic_persist_fs::StockFs;

use crate::{SandboxConfig, SandboxError, SandboxResult};

/// A sandboxed Stock implementation that maintains changes in a delta layer
/// over a read-only base layer.
#[derive(Debug)]
pub struct SandboxStock {
    /// Read-only reference to the base storage
    base: StockFs,
    /// Writable delta storage for incremental changes  
    delta: StockFs,
}

impl SandboxStock {
    /// Create a new sandbox stock with an existing base and a new delta storage
    pub fn create_with_base(config: SandboxConfig, base: StockFs) -> SandboxResult<Self> {
        // Create a new empty delta storage with the same articles and initial state as base
        let articles = base.articles().clone();
        let state = base.state().clone();
        let delta = StockFs::new(articles, state, config.delta_path)
            .map_err(SandboxError::DeltaStorage)?;
            
        Ok(Self { base, delta })
    }
    
    /// Create a sandbox by loading existing base storage and creating new delta
    pub fn load_base_create_delta(config: SandboxConfig) -> SandboxResult<Self> {
        // Load the existing base storage
        let base = StockFs::load(config.base_path)?;
        
        // Create new delta storage with same initial state
        let articles = base.articles().clone();
        let state = base.state().clone();
        let delta = StockFs::new(articles, state, config.delta_path)
            .map_err(SandboxError::DeltaStorage)?;
            
        Ok(Self { base, delta })
    }
    
    /// Get a reference to the base storage (read-only)
    pub fn base(&self) -> &StockFs {
        &self.base
    }
    
    /// Get a reference to the delta storage
    pub fn delta(&self) -> &StockFs {
        &self.delta
    }
    
    /// Get a mutable reference to the delta storage
    pub fn delta_mut(&mut self) -> &mut StockFs {
        &mut self.delta
    }
    
    /// Commit the delta changes to the base storage
    /// 
    /// This method would copy all changes from delta to base storage.
    /// For now, this is a conceptual method - in a real implementation,
    /// you would iterate through all delta operations and apply them to base.
    pub fn commit_to_base(&mut self) -> SandboxResult<()> {
        // TODO: Implement proper delta merging
        // This would involve:
        // 1. Iterating through all operations in delta
        // 2. Adding them to base
        // 3. Merging state changes
        // 4. Clearing delta
        
        self.delta.commit_transaction();
        Ok(())
    }
    
    /// Discard all changes in the delta storage
    pub fn rollback(&mut self) -> SandboxResult<()> {
        // For a full rollback, we would need to recreate the delta storage
        // or implement an abort_transaction method
        // For now, we'll reset the delta to match base initial state
        
        let articles = self.base.articles().clone();
        let state = self.base.state().clone();
        let path = self.delta.config();
        
        // Recreate delta storage to effectively clear it
        self.delta = StockFs::new(articles, state, path)
            .map_err(SandboxError::DeltaStorage)?;
            
        Ok(())
    }
}

impl Stock for SandboxStock {
    type Conf = SandboxConfig;
    type Error = SandboxError;

    fn new(articles: Articles, state: EffectiveState, config: SandboxConfig) -> SandboxResult<Self> {
        // Create both base and delta storages
        let base = StockFs::new(articles.clone(), state.clone(), config.base_path)?;
        let delta = StockFs::new(articles, state, config.delta_path)
            .map_err(SandboxError::DeltaStorage)?;
            
        Ok(Self { base, delta })
    }

    fn load(config: SandboxConfig) -> SandboxResult<Self> {
        Self::load_base_create_delta(config)
    }

    fn config(&self) -> SandboxConfig {
        SandboxConfig::new(self.base.config(), self.delta.config())
    }

    // Read operations: Check delta first, then base
    fn articles(&self) -> &Articles {
        // For articles, delta takes precedence but they should be the same
        self.delta.articles()
    }

    fn state(&self) -> &EffectiveState {
        // For state, delta reflects current working state
        self.delta.state()
    }

    fn is_valid(&self, opid: Opid) -> bool {
        // Check delta first for validity info
        if self.delta.has_operation(opid) {
            self.delta.is_valid(opid)
        } else {
            self.base.is_valid(opid)
        }
    }

    fn has_operation(&self, opid: Opid) -> bool {
        self.delta.has_operation(opid) || self.base.has_operation(opid)
    }

    fn operation_count(&self) -> u64 {
        // This is approximate - could have overlaps between base and delta
        // In a real implementation, we'd track unique operations
        self.delta.operation_count() + self.base.operation_count()
    }

    fn operation(&self, opid: Opid) -> Operation {
        if self.delta.has_operation(opid) {
            self.delta.operation(opid)
        } else {
            self.base.operation(opid)
        }
    }

    fn operations(&self) -> impl Iterator<Item = (Opid, Operation)> {
        // Combine both iterators, with delta taking precedence
        // This is a simplified implementation
        self.delta.operations().chain(
            self.base.operations().filter(|(opid, _)| !self.delta.has_operation(*opid))
        )
    }

    fn transition(&self, opid: Opid) -> Transition {
        if self.delta.has_operation(opid) {
            self.delta.transition(opid)
        } else {
            self.base.transition(opid)
        }
    }

    fn trace(&self) -> impl Iterator<Item = (Opid, Transition)> {
        // Similar to operations, combine with delta precedence
        self.delta.trace().chain(
            self.base.trace().filter(|(opid, _)| !self.delta.has_operation(*opid))
        )
    }

    fn read_by(&self, addr: CellAddr) -> impl Iterator<Item = Opid> {
        // Combine read relationships from both storages
        self.delta.read_by(addr).chain(self.base.read_by(addr))
    }

    fn spent_by(&self, addr: CellAddr) -> Option<Opid> {
        // Delta takes precedence for spending information
        self.delta.spent_by(addr).or_else(|| self.base.spent_by(addr))
    }

    // Write operations: Only affect delta storage
    fn mark_valid(&mut self, opid: Opid) {
        self.delta.mark_valid(opid)
    }

    fn mark_invalid(&mut self, opid: Opid) {
        self.delta.mark_invalid(opid)
    }

    fn update_articles(
        &mut self,
        f: impl FnOnce(&mut Articles) -> Result<bool, SemanticError>,
    ) -> Result<bool, MultiError<SemanticError, SandboxError>> {
        self.delta.update_articles(f)
            .map_err(|err| match err {
                MultiError::A(semantic_err) => MultiError::A(semantic_err),
                MultiError::B(fs_err) => MultiError::B(SandboxError::DeltaStorage(fs_err)),
                MultiError::C(_) => unreachable!(),
            })
    }

    fn update_state<R>(&mut self, f: impl FnOnce(&mut EffectiveState, &Articles) -> R) -> Result<R, SandboxError> {
        self.delta.update_state(f)
            .map_err(SandboxError::DeltaStorage)
    }

    fn add_operation(&mut self, opid: Opid, operation: &Operation) {
        self.delta.add_operation(opid, operation)
    }

    fn add_transition(&mut self, opid: Opid, transition: &Transition) {
        self.delta.add_transition(opid, transition)
    }

    fn add_reading(&mut self, addr: CellAddr, reader: Opid) {
        self.delta.add_reading(addr, reader)
    }

    fn add_spending(&mut self, spent: CellAddr, spender: Opid) {
        self.delta.add_spending(spent, spender)
    }

    fn commit_transaction(&mut self) {
        self.delta.commit_transaction()
    }
}
