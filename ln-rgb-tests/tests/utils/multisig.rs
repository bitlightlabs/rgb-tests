// MultiSig helper utilities for 2-2 multisig testing
//
// Provides helper functions to create and manage 2-2 multisig transactions

use bitcoin::bip32::Xpriv;
use bitcoin::secp256k1::{All, PublicKey, Secp256k1};
use bitcoin::{Address, Network, ScriptBuf, Transaction, Witness, TxOut, TxIn, OutPoint};
use bitcoin::blockdata::script::Builder;
use bitcoin::blockdata::opcodes;
use bitcoin::sighash::{EcdsaSighashType, SighashCache};
use bitcoin::Amount;
use bitcoin::transaction::Version;
use bitcoin::absolute::LockTime;
use bitcoin::Sequence;

#[derive(Clone)]
pub struct MultiSigHelper {
    alice_xprv: Xpriv,
    bob_xprv: Xpriv,
    secp: Secp256k1<All>,
}

impl MultiSigHelper {
    /// Create new MultiSigHelper with two master xprivs
    pub fn new(alice_xprv: Xpriv, bob_xprv: Xpriv) -> Self {
        Self {
            alice_xprv,
            bob_xprv,
            secp: Secp256k1::new(),
        }
    }

    /// Derive public key from xpriv
    fn derive_pubkey(&self, xprv: &Xpriv) -> PublicKey {
        let private_key = xprv.private_key;
        PublicKey::from_secret_key(&self.secp, &private_key)
    }

    /// Get Alice's public key for shutdown address
    pub fn derive_alice_pubkey(&self) -> PublicKey {
        self.derive_pubkey(&self.alice_xprv)
    }

    /// Get Bob's public key for shutdown address
    pub fn derive_bob_pubkey(&self) -> PublicKey {
        self.derive_pubkey(&self.bob_xprv)
    }

    /// Get Alice's xpriv for signing
    pub fn alice_xprv(&self) -> &Xpriv {
        &self.alice_xprv
    }

    /// Get Bob's xpriv for signing
    pub fn bob_xprv(&self) -> &Xpriv {
        &self.bob_xprv
    }

    /// Create 2-of-2 multisig script
    ///
    /// Format: OP_2 <pubkey1> <pubkey2> OP_2 OP_CHECKMULTISIG
    pub fn create_2of2_script(&self) -> bitcoin::ScriptBuf {
        let alice_secp_pubkey = self.derive_pubkey(&self.alice_xprv);
        let bob_secp_pubkey = self.derive_pubkey(&self.bob_xprv);

        // Convert secp256k1::PublicKey to bitcoin::PublicKey
        let alice_pubkey = bitcoin::PublicKey::new(alice_secp_pubkey);
        let bob_pubkey = bitcoin::PublicKey::new(bob_secp_pubkey);

        Builder::new()
            .push_int(2)
            .push_key(&alice_pubkey)
            .push_key(&bob_pubkey)
            .push_int(2)
            .push_opcode(opcodes::all::OP_CHECKMULTISIG)
            .into_script()
    }

    /// Create P2WSH multisig address
    pub fn create_multisig_address(&self, network: Network) -> Address {
        let script = self.create_2of2_script();
        Address::p2wsh(&script, network)
    }

    /// Create funding transaction: single-sig UTXO → multisig address
    ///
    /// # Arguments
    /// * `input_outpoint` - The UTXO to spend (from Alice's wallet)
    /// * `input_amount` - Amount in the input UTXO
    /// * `output_amount` - Amount to send to multisig (input_amount - fee)
    /// * `multisig_address` - The P2WSH multisig address
    ///
    /// # Returns
    /// Unsigned transaction (caller must sign with wallet)
    pub fn create_funding_tx(
        input_outpoint: OutPoint,
        input_amount: Amount,
        output_amount: Amount,
        multisig_address: Address,
    ) -> Transaction {
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: input_outpoint,
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: output_amount,
                script_pubkey: multisig_address.script_pubkey(),
            }],
        }
    }

    /// Create spending transaction: multisig UTXO → destination address
    ///
    /// # Arguments
    /// * `multisig_outpoint` - The multisig UTXO to spend
    /// * `multisig_amount` - Amount in the multisig UTXO
    /// * `output_amount` - Amount to send to destination (multisig_amount - fee)
    /// * `destination` - The destination address
    ///
    /// # Returns
    /// Unsigned transaction (caller must call finalize_tx to sign)
    pub fn create_spending_tx(
        multisig_outpoint: OutPoint,
        multisig_amount: Amount,
        output_amount: Amount,
        destination: Address,
    ) -> Transaction {
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: multisig_outpoint,
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: output_amount,
                script_pubkey: destination.script_pubkey(),
            }],
        }
    }

    /// Sign P2WPKH transaction with a single xpriv
    ///
    /// Signs input[0] with the provided xpriv
    pub fn sign_p2wpkh_tx(
        &self,
        tx: &mut Transaction,
        xprv: &Xpriv,
        input_amount: Amount,
    ) -> Result<(), String> {
        if tx.input.is_empty() {
            return Err("Transaction has no inputs".to_string());
        }

        // Derive pubkey
        let pubkey = self.derive_pubkey(xprv);
        let bitcoin_pubkey = bitcoin::PublicKey::new(pubkey);
        let compressed_pubkey = bitcoin::CompressedPublicKey::try_from(bitcoin_pubkey)
            .map_err(|e| format!("Failed to compress pubkey: {}", e))?;

        // Create P2WPKH script_pubkey (OP_0 <20-byte-pubkey-hash>)
        let script_pubkey = ScriptBuf::new_p2wpkh(&compressed_pubkey.wpubkey_hash());

        // Create sighash for input[0]
        let mut sighash_cache = SighashCache::new(tx.clone());
        let sighash = sighash_cache
            .p2wpkh_signature_hash(
                0, // input_index
                &script_pubkey,
                input_amount,
                EcdsaSighashType::All,
            )
            .map_err(|e| format!("Failed to compute sighash: {}", e))?;

        // Sign
        let private_key = &xprv.private_key;
        let msg = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
            .map_err(|e| format!("Failed to create message: {}", e))?;
        let sig = self.secp.sign_ecdsa(&msg, private_key);
        let mut sig_bytes = sig.serialize_der().to_vec();
        sig_bytes.push(EcdsaSighashType::All as u8);

        // Assemble witness for P2WPKH: [signature, pubkey]
        let witness = Witness::from_slice(&[
            sig_bytes,
            compressed_pubkey.to_bytes().to_vec(),
        ]);

        // Set witness for input[0]
        tx.input[0].witness = witness;

        Ok(())
    }

    /// Sign P2WPKH transaction with a single xpriv (static version)
    ///
    /// Static version that doesn't require MultiSigHelper instance.
    /// Signs input[0] with the provided xpriv.
    pub fn sign_p2wpkh_tx_static(
        tx: &mut Transaction,
        xprv: Xpriv,
        input_amount: Amount,
    ) -> Result<(), String> {
        if tx.input.is_empty() {
            return Err("Transaction has no inputs".to_string());
        }

        let secp = bitcoin::secp256k1::Secp256k1::new();

        // Derive pubkey from xpriv at path m/0
        use bitcoin::bip32::ChildNumber;
        let child_xprv = xprv
            .derive_priv(&secp, &[ChildNumber::from_normal_idx(0).unwrap()])
            .map_err(|e| format!("Failed to derive child key: {}", e))?;

        let pubkey = child_xprv.to_priv().public_key(&secp);
        let compressed_pubkey = bitcoin::CompressedPublicKey::try_from(pubkey)
            .map_err(|e| format!("Failed to compress pubkey: {}", e))?;

        // Create P2WPKH script_pubkey
        let script_pubkey = ScriptBuf::new_p2wpkh(&compressed_pubkey.wpubkey_hash());

        // Create sighash for input[0]
        let mut sighash_cache = SighashCache::new(tx.clone());
        let sighash = sighash_cache
            .p2wpkh_signature_hash(
                0,
                &script_pubkey,
                input_amount,
                EcdsaSighashType::All,
            )
            .map_err(|e| format!("Failed to compute sighash: {}", e))?;

        // Sign
        let private_key = &child_xprv.private_key;
        let msg = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
            .map_err(|e| format!("Failed to create message: {}", e))?;
        let sig = secp.sign_ecdsa(&msg, private_key);
        let mut sig_bytes = sig.serialize_der().to_vec();
        sig_bytes.push(EcdsaSighashType::All as u8);

        // Assemble witness for P2WPKH
        let witness = Witness::from_slice(&[
            sig_bytes,
            compressed_pubkey.to_bytes().to_vec(),
        ]);

        tx.input[0].witness = witness;

        Ok(())
    }

    /// Sign transaction input and assemble witness
    ///
    /// Signs input[0] with both Alice and Bob keys, then assembles the witness
    pub fn finalize_tx(
        &self,
        tx: &mut Transaction,
        multisig_script: &ScriptBuf,
        input_amount: Amount,
    ) -> Result<(), String> {
        if tx.input.is_empty() {
            return Err("Transaction has no inputs".to_string());
        }

        // Create sighash for input[0]
        let mut sighash_cache = SighashCache::new(tx.clone());
        let sighash = sighash_cache
            .p2wsh_signature_hash(
                0, // input_index
                multisig_script,
                input_amount,
                EcdsaSighashType::All,
            )
            .map_err(|e| format!("Failed to compute sighash: {}", e))?;

        // Sign with Alice
        let alice_private_key = &self.alice_xprv.private_key;
        let alice_msg = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
            .map_err(|e| format!("Failed to create message: {}", e))?;
        let alice_sig = self.secp.sign_ecdsa(&alice_msg, alice_private_key);
        let mut alice_sig_bytes = alice_sig.serialize_der().to_vec();
        alice_sig_bytes.push(EcdsaSighashType::All as u8);

        // Sign with Bob
        let bob_private_key = &self.bob_xprv.private_key;
        let bob_msg = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
            .map_err(|e| format!("Failed to create message: {}", e))?;
        let bob_sig = self.secp.sign_ecdsa(&bob_msg, bob_private_key);
        let mut bob_sig_bytes = bob_sig.serialize_der().to_vec();
        bob_sig_bytes.push(EcdsaSighashType::All as u8);

        // Assemble witness (OP_CHECKMULTISIG bug requires empty element)
        let witness = Witness::from_slice(&[
            Vec::new(),                     // OP_CHECKMULTISIG bug dummy element
            alice_sig_bytes,                // Alice signature
            bob_sig_bytes,                  // Bob signature
            multisig_script.to_bytes(),     // Redeem script
        ]);

        // Set witness for input[0]
        tx.input[0].witness = witness;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::Network;

    #[test]
    fn test_create_2of2_script() {
        let alice_xprv = Xpriv::new_master(Network::Regtest, &[0u8; 64]).unwrap();
        let bob_xprv = Xpriv::new_master(Network::Regtest, &[1u8; 64]).unwrap();

        let multisig = MultiSigHelper::new(alice_xprv, bob_xprv);
        let script = multisig.create_2of2_script();

        assert!(!script.is_empty());
        assert!(script.is_op_return() == false);
    }

    #[test]
    fn test_create_multisig_address() {
        let alice_xpriv = Xpriv::new_master(Network::Regtest, &[0u8; 64]).unwrap();
        let bob_xpriv = Xpriv::new_master(Network::Regtest, &[1u8; 64]).unwrap();

        let multisig = MultiSigHelper::new(alice_xpriv, bob_xpriv);
        let address = multisig.create_multisig_address(Network::Regtest);

        // P2WSH address should start with "bcrt1" on regtest
        let addr_str = address.to_string();
        assert!(addr_str.starts_with("bcrt1"));
    }
}
