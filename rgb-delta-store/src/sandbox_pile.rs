// RGB Sandbox Pile Implementation
//
// This implements a sandboxed version of the Pile trait that uses a delta-over-base
// storage pattern for witness and seal data.

use std::collections::HashSet;
use amplify::confinement::SmallOrdMap;
use rgb::{Pile, RgbSeal, Witness, WitnessStatus, OpRels};
use hypersonic::CellAddr;
use hypersonic::Opid;
use rgb_persist_fs::PileFs;

use crate::{SandboxConfig, SandboxError, SandboxResult};

/// A sandboxed Pile implementation that maintains witness and seal changes
/// in a delta layer over a read-only base layer.
// Debug removed due to associated type constraints
pub struct SandboxPile<Seal: RgbSeal>
where 
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>
{
    /// Read-only reference to the base pile storage
    base: PileFs<Seal>,
    /// Writable delta pile storage for incremental changes
    delta: PileFs<Seal>,
    /// Configuration for recreating the delta storage
    config: SandboxConfig,
}

impl<Seal: RgbSeal> SandboxPile<Seal>
where 
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>,
    Seal::Client: strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::Published: Eq + strict_encoding::StrictEncode + strict_encoding::StrictDecode,
{
    /// Create a new sandbox pile with an existing base and a new delta storage
    pub fn create_with_base(config: SandboxConfig, base: PileFs<Seal>) -> SandboxResult<Self> {
        let delta = PileFs::new(config.delta_path.clone())?;
        Ok(Self { base, delta, config })
    }
    
    /// Create a sandbox by loading existing base storage and creating new delta
    pub fn load_base_create_delta(config: SandboxConfig) -> SandboxResult<Self> {
        let base = PileFs::load(config.base_path.clone())?;
        let delta = PileFs::new(config.delta_path.clone())?;
        Ok(Self { base, delta, config })
    }
    
    /// Get a reference to the base storage (read-only)
    pub fn base(&self) -> &PileFs<Seal> {
        &self.base
    }
    
    /// Get a reference to the delta storage
    pub fn delta(&self) -> &PileFs<Seal> {
        &self.delta
    }
    
    /// Get a mutable reference to the delta storage
    pub fn delta_mut(&mut self) -> &mut PileFs<Seal> {
        &mut self.delta
    }
    
    /// Commit the delta changes to the base storage
    /// 
    /// This method merges all changes from delta pile to base pile.
    /// Witnesses, seals, and status updates are copied from delta to base.
    pub fn commit_to_base(&mut self) -> SandboxResult<()> {
        // Step 1: Ensure delta is in consistent state
        self.delta.commit_transaction();
        
        // Step 2: Copy all witnesses from delta to base
        for witness in self.delta.witnesses() {
            // Add witness with all its associated data
            // Use the first operation ID from the set as primary key
            let first_opid = witness.opids.iter().next()
                .copied()
                .expect("Witness should have at least one operation ID");
                
            self.base.add_witness(
                first_opid,
                witness.id,
                &witness.published,
                &witness.client,
                witness.status
            );
            
            // Add seals for all operations this witness covers
            for opid in &witness.opids {
                let seals = self.delta.seals(*opid, u16::MAX);
                if !seals.is_empty() {
                    self.base.add_seals(*opid, seals);
                }
            }
        }
        
        // Step 3: Sync witness status updates
        for witness_id in self.delta.witness_ids() {
            let status = self.delta.witness_status(witness_id);
            self.base.update_witness_status(witness_id, status);
        }
        
        // Step 4: Commit all changes to base
        self.base.commit_transaction();
        
        // Step 5: Reset delta to clean state
        self.delta = PileFs::new(self.config.delta_path.clone())?;
        
        Ok(())
    }
    
    /// Discard all changes in the delta storage
    pub fn rollback(&mut self) -> SandboxResult<()> {
        // Recreate delta storage to effectively clear it
        self.delta = PileFs::new(self.config.delta_path.clone())?;
        Ok(())
    }
}

impl<Seal: RgbSeal> Pile for SandboxPile<Seal>
where
    Seal::WitnessId: From<[u8; 32]> + Into<[u8; 32]>,
    Seal::Client: strict_encoding::StrictEncode + strict_encoding::StrictDecode,
    Seal::Published: Eq + strict_encoding::StrictEncode + strict_encoding::StrictDecode,
{
    type Seal = Seal;
    type Conf = SandboxConfig;
    type Error = SandboxError;

    fn new(config: SandboxConfig) -> SandboxResult<Self> {
        let base = PileFs::new(config.base_path.clone())?;
        let delta = PileFs::new(config.delta_path.clone())?;
        Ok(Self { base, delta, config })
    }

    fn load(config: SandboxConfig) -> SandboxResult<Self> {
        Self::load_base_create_delta(config)
    }

    fn pub_witness(&self, wid: Seal::WitnessId) -> Seal::Published {
        // Check delta first, then base
        if self.delta.has_witness(wid) {
            self.delta.pub_witness(wid)
        } else {
            self.base.pub_witness(wid)
        }
    }

    fn has_witness(&self, wid: Seal::WitnessId) -> bool {
        self.delta.has_witness(wid) || self.base.has_witness(wid)
    }

    fn cli_witness(&self, wid: Seal::WitnessId) -> Seal::Client {
        if self.delta.has_witness(wid) {
            self.delta.cli_witness(wid)
        } else {
            self.base.cli_witness(wid)
        }
    }

    fn witness_status(&self, wid: Seal::WitnessId) -> WitnessStatus {
        // Delta takes precedence for status updates
        if self.delta.has_witness(wid) {
            self.delta.witness_status(wid)
        } else {
            self.base.witness_status(wid)
        }
    }

    fn witness_ids(&self) -> impl Iterator<Item = Seal::WitnessId> {
        // Combine witness IDs from both storages, avoiding duplicates
        let base_ids: HashSet<_> = self.base.witness_ids().collect();
        let delta_ids: HashSet<_> = self.delta.witness_ids().collect();
        base_ids.union(&delta_ids).cloned().collect::<Vec<_>>().into_iter()
    }

    fn witnesses(&self) -> impl Iterator<Item = Witness<Self::Seal>> {
        // This is complex to implement properly due to the need to merge data
        // For now, we'll return delta witnesses followed by base witnesses not in delta
        let delta_wids: HashSet<_> = self.delta.witness_ids().collect();
        
        self.delta.witnesses().chain(
            self.base.witnesses().filter(move |w| {
                // This closure captures witness IDs from delta to avoid duplicates
                // In a real implementation, we'd need a more sophisticated approach
                !delta_wids.contains(&w.id)
            })
        )
    }

    fn op_witness_ids(&self, opid: Opid) -> impl ExactSizeIterator<Item = Seal::WitnessId> {
        // Combine witness IDs from both storages for the given operation
        let mut delta_ids: Vec<_> = self.delta.op_witness_ids(opid).collect();
        let base_ids: Vec<_> = self.base.op_witness_ids(opid).collect();
        
        // Remove duplicates by extending delta with base IDs not already present
        for base_id in base_ids {
            if !delta_ids.contains(&base_id) {
                delta_ids.push(base_id);
            }
        }
        
        delta_ids.into_iter()
    }

    fn ops_by_witness_id(&self, wid: Seal::WitnessId) -> impl ExactSizeIterator<Item = Opid> {
        // Similar approach: combine operations from both storages
        let mut delta_ops: Vec<_> = self.delta.ops_by_witness_id(wid).collect();
        let base_ops: Vec<_> = self.base.ops_by_witness_id(wid).collect();
        
        for base_op in base_ops {
            if !delta_ops.contains(&base_op) {
                delta_ops.push(base_op);
            }
        }
        
        delta_ops.into_iter()
    }

    fn seal(&self, addr: CellAddr) -> Option<Seal::Definition> {
        // Delta takes precedence for seal definitions
        self.delta.seal(addr).or_else(|| self.base.seal(addr))
    }

    fn seals(&self, opid: Opid, up_to: u16) -> SmallOrdMap<u16, Seal::Definition> {
        // Merge seals from both storages, with delta taking precedence
        let mut merged_seals = self.base.seals(opid, up_to);
        let delta_seals = self.delta.seals(opid, up_to);
        
        // Delta seals override base seals
        for (key, value) in delta_seals {
            let _ = merged_seals.insert(key, value);
        }
        
        merged_seals
    }

    fn op_relations(&self, opid: Opid, up_to: u16) -> OpRels<Self::Seal> {
        // This requires merging OpRels data from both storages
        // For simplicity, we'll check if delta has witness IDs for this operation
        if !self.delta.op_witness_ids(opid).collect::<Vec<_>>().is_empty() {
            self.delta.op_relations(opid, up_to)
        } else {
            self.base.op_relations(opid, up_to)
        }
    }

    // Write operations: Only affect delta storage
    fn add_witness(
        &mut self,
        opid: Opid,
        wid: Seal::WitnessId,
        published: &Seal::Published,
        anchor: &Seal::Client,
        status: WitnessStatus,
    ) {
        self.delta.add_witness(opid, wid, published, anchor, status)
    }

    fn add_seals(&mut self, opid: Opid, seals: SmallOrdMap<u16, Seal::Definition>) {
        self.delta.add_seals(opid, seals)
    }

    fn update_witness_status(&mut self, wid: Seal::WitnessId, status: WitnessStatus) {
        self.delta.update_witness_status(wid, status)
    }

    fn commit_transaction(&mut self) {
        self.delta.commit_transaction()
    }
}