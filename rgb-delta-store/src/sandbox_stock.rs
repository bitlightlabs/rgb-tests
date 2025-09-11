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
    /// Rollback state: when true, delta operations are logically ignored
    is_rolled_back: bool,
    /// Snapshot of delta operations at rollback point (for potential recovery)
    rollback_snapshot: Option<u64>,
}

impl SandboxStock {
    /// Create a new sandbox stock with an existing base and a new delta storage
    pub fn create_with_base(config: SandboxConfig, base: StockFs) -> SandboxResult<Self> {
        // Create a new empty delta storage with the same articles and initial state as base
        let articles = base.articles().clone();
        let state = base.state().clone();
        let delta = StockFs::new(articles, state, config.delta_path)
            .map_err(SandboxError::DeltaStorage)?;
            
        Ok(Self { 
            base, 
            delta, 
            is_rolled_back: false,
            rollback_snapshot: None,
        })
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
            
        Ok(Self { 
            base, 
            delta, 
            is_rolled_back: false,
            rollback_snapshot: None,
        })
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
    /// This method merges all changes from delta to base storage.
    /// After this operation, the delta becomes empty and all changes
    /// are permanently committed to the base layer.
    pub fn commit_to_base(&mut self) -> SandboxResult<()> {
        // Step 1: Ensure delta is in consistent state
        self.delta.commit_transaction();
        
        // Step 2: Copy all operations from delta to base
        for (opid, operation) in self.delta.operations() {
            self.base.add_operation(opid, &operation);
        }
        
        // Step 3: Copy all transitions from delta to base  
        for (opid, transition) in self.delta.trace() {
            self.base.add_transition(opid, &transition);
        }
        
        // Step 4: Merge validity and spending information
        // Note: This is simplified - in practice we'd need to handle conflicts
        for (opid, _) in self.delta.operations() {
            if self.delta.is_valid(opid) {
                self.base.mark_valid(opid);
            } else {
                self.base.mark_invalid(opid);
            }
        }
        
        // Step 5: Update base state to match delta state
        let delta_state = self.delta.state().clone();
        self.base.update_state(|state, _| {
            *state = delta_state;
        }).map_err(SandboxError::BaseStorage)?;
        
        // Step 6: Commit changes to base
        self.base.commit_transaction();
        
        // Step 7: Reset delta to clean state
        // Create fresh delta with same config but empty state
        let articles = self.base.articles().clone();
        let initial_state = self.base.state().clone();
        let config = SandboxConfig::new(self.base.config(), self.delta.config());
        
        self.delta = StockFs::new(articles, initial_state, config.delta_path)
            .map_err(SandboxError::DeltaStorage)?;
        
        Ok(())
    }
    
    /// Logically rollback all delta changes without physically deleting AORA data
    /// 
    /// Based on our AORA/AURA analysis, we implement rollback through logical isolation:
    /// - AORA data (stash, trace, read) is kept for audit/history - never deleted
    /// - AURA data (spent, valid) can be reset through transaction management  
    /// - Application layer controls visibility of delta operations
    pub fn rollback(&mut self) -> SandboxResult<()> {
        println!("🔄 Performing AORA-compatible logical rollback...");
        
        // Step 1: Mark as rolled back (logical isolation)
        self.is_rolled_back = true;
        self.rollback_snapshot = Some(self.delta.operation_count());
        
        // Step 2: Reset state to base state (this is safe)
        let base_state = self.base.state().clone();
        self.delta.update_state(|state, _| {
            *state = base_state;
        }).map_err(SandboxError::DeltaStorage)?;
        
        // Step 3: Commit the state reset (AURA transaction)
        self.delta.commit_transaction();
        
        println!("✅ Logical rollback completed - delta operations hidden, AORA history preserved");
        Ok(())
    }
    
    /// Re-enable delta operations after rollback
    pub fn resume_delta_operations(&mut self) -> SandboxResult<()> {
        self.is_rolled_back = false;
        self.rollback_snapshot = None;
        Ok(())
    }
    
    /// Check if currently in rolled-back state
    pub fn is_rolled_back(&self) -> bool {
        self.is_rolled_back
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
            
        Ok(Self { 
            base, 
            delta, 
            is_rolled_back: false,
            rollback_snapshot: None,
        })
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
        if self.is_rolled_back {
            // Rollback state: only query base, ignore delta
            self.base.has_operation(opid)
        } else {
            // Normal state: delta-over-base
            self.delta.has_operation(opid) || self.base.has_operation(opid)
        }
    }

    fn operation_count(&self) -> u64 {
        // This is approximate - could have overlaps between base and delta
        // In a real implementation, we'd track unique operations
        self.delta.operation_count() + self.base.operation_count()
    }

    fn operation(&self, opid: Opid) -> Operation {
        if self.is_rolled_back {
            // Rollback state: only query base
            self.base.operation(opid)
        } else if self.delta.has_operation(opid) {
            // Normal state: delta takes precedence
            self.delta.operation(opid)
        } else {
            self.base.operation(opid)
        }
    }

    fn operations(&self) -> impl Iterator<Item = (Opid, Operation)> {
        if self.is_rolled_back {
            // Rollback state: only return base operations
            Box::new(self.base.operations()) as Box<dyn Iterator<Item = (Opid, Operation)>>
        } else {
            // Normal state: combine both iterators, with delta taking precedence
            Box::new(self.delta.operations().chain(
                self.base.operations().filter(|(opid, _)| !self.delta.has_operation(*opid))
            )) as Box<dyn Iterator<Item = (Opid, Operation)>>
        }
    }

    fn transition(&self, opid: Opid) -> Transition {
        if self.is_rolled_back {
            // Rollback state: only query base
            self.base.transition(opid)
        } else if self.delta.has_operation(opid) {
            // Normal state: delta takes precedence
            self.delta.transition(opid)
        } else {
            self.base.transition(opid)
        }
    }

    fn trace(&self) -> impl Iterator<Item = (Opid, Transition)> {
        if self.is_rolled_back {
            // Rollback state: only return base trace
            Box::new(self.base.trace()) as Box<dyn Iterator<Item = (Opid, Transition)>>
        } else {
            // Normal state: combine with delta precedence
            Box::new(self.delta.trace().chain(
                self.base.trace().filter(|(opid, _)| !self.delta.has_operation(*opid))
            )) as Box<dyn Iterator<Item = (Opid, Transition)>>
        }
    }

    fn read_by(&self, addr: CellAddr) -> impl Iterator<Item = Opid> {
        if self.is_rolled_back {
            // Rollback state: only return base read relationships
            Box::new(self.base.read_by(addr)) as Box<dyn Iterator<Item = Opid>>
        } else {
            // Normal state: combine read relationships from both storages
            Box::new(self.delta.read_by(addr).chain(self.base.read_by(addr))) as Box<dyn Iterator<Item = Opid>>
        }
    }

    fn spent_by(&self, addr: CellAddr) -> Option<Opid> {
        if self.is_rolled_back {
            // Rollback state: only query base spending information
            self.base.spent_by(addr)
        } else {
            // Normal state: delta takes precedence for spending information
            self.delta.spent_by(addr).or_else(|| self.base.spent_by(addr))
        }
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
