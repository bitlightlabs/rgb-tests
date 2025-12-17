// Test helper functions for ln-rgb-tests
//
// Provides utility functions to simplify test code

use super::*;
use anyhow::{Context, Result};
use bp::Outpoint;
use rgb::ContractId;
use std::sync::Arc;
use lightning_rgb_types::{convert_tx_from_bitcoin_to_bp, convert_psbt_from_bitcoin_to_bp, convert_psbt_from_bp_to_bitcoin, contract_id_from_bytes};
use lightning_rgb_ln_types::{AssetId, AssetQuantity};
use lightning_rgb_wallet::RgbFundingRequest;
use crate::utils::asset_params::NIAIssueParams;
use crate::utils::multisig::MultiSigHelper;

/// Get a UTXO for the wallet by funding it with Bitcoin
///
/// This mirrors the behavior of rgb-tests TestWallet::get_utxo:
/// 1. Get a new address from the wallet
/// 2. Send Bitcoin to that address
/// 3. Wait for confirmation
/// 4. Sync wallet
/// 5. Return the funded UTXO outpoint
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `sats` - Amount of sats to fund (optional, defaults to 10_000)
/// * `instance` - Bitcoin node instance to use for funding
///
/// # Returns
/// The outpoint of the funded UTXO
pub async fn get_utxo(
    controller: &Arc<WalletController>,
    sats: Option<u64>,
    instance: u8,
) -> Result<Outpoint> {
    let sats = sats.unwrap_or(10_000);

    // 1. Get a new address from the wallet
    let address = controller.get_address().await?;
    println!("  Funding address: {}", address);

    // 2. Send Bitcoin to the address
    let txid = chain::fund_wallet(address.to_string(), Some(sats), instance);
    println!("  Funding txid: {}", txid);

    // 3. Wait for confirmation (fund_wallet already mines 1 block)
    assert!(
        chain::is_tx_confirmed(&txid, instance),
        "Funding transaction should be confirmed"
    );

    // 4. Sync wallet to see the new UTXO
    controller.sync_runtime().await
        .context("Failed to sync wallet runtime")?;

    // 5. Find and return the funded UTXO
    let utxos = controller.list_utxos().await
        .context("Failed to list UTXOs")?;
    println!("  Wallet has {} UTXOs after funding", utxos.len());

    // Find the UTXO from our funding transaction
    for utxo in utxos {
        let utxo_txid_str = utxo.txid.to_string();
        if utxo_txid_str.starts_with(&txid[..8]) {
            // Match by first 8 chars of txid
            println!("  Found funded UTXO: {}:{}", utxo.txid, utxo.vout);
            return Ok(utxo);
        }
    }

    anyhow::bail!("Failed to find funded UTXO for txid {} in wallet", txid)
}

/// Check if allocations match expected amounts
///
/// This is a simplified version of rgb-tests TestWallet::check_allocations
/// that only verifies amounts (sorted), not specific UTXOs.
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `contract_id` - The contract to check
/// * `expected` - Expected allocation amounts (will be sorted)
pub async fn check_allocations(
    controller: &Arc<WalletController>,
    contract_id: ContractId,
    mut expected: Vec<u64>,
) -> Result<()> {
    let allocations = controller.get_allocations(contract_id).await
        .context("Failed to get allocations")?;

    // Extract amounts and sort
    let mut actual: Vec<u64> = allocations.iter().map(|(_, amount)| *amount).collect();
    actual.sort();
    expected.sort();

    if actual == expected {
        println!("  ✓ Allocations match: {:?}", actual);
        Ok(())
    } else {
        anyhow::bail!(
            "Allocation mismatch for contract {}: expected {:?}, got {:?}",
            contract_id, expected, actual
        )
    }
}

/// Check if total allocation sum matches expected amount
///
/// This is useful when allocations may be merged/consolidated by the RGB runtime,
/// and we only care about the total balance, not the specific allocation distribution.
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `contract_id` - The contract to check
/// * `expected_sum` - Expected total amount
pub async fn check_allocations_sum(
    controller: &Arc<WalletController>,
    contract_id: ContractId,
    expected_sum: u64,
) -> Result<()> {
    let allocations = controller.get_allocations(contract_id).await
        .context("Failed to get allocations")?;

    // Calculate total sum
    let actual_sum: u64 = allocations.iter().map(|(_, amount)| *amount).sum();

    if actual_sum == expected_sum {
        println!("  ✓ Total allocation sum matches: {}", actual_sum);
        Ok(())
    } else {
        anyhow::bail!(
            "Allocation sum mismatch for contract {}: expected {}, got {} (allocations: {:?})",
            contract_id,
            expected_sum,
            actual_sum,
            allocations.iter().map(|(_, amt)| amt).collect::<Vec<_>>()
        )
    }
}

/// Get current blockchain height
///
/// Returns the current block count from the regtest bitcoin node
pub async fn get_height() -> u64 {
    super::chain::get_block_count().await
}

/// Share contract between two wallets
///
/// Exports the contract from sender's wallet and imports it to receiver's wallet.
/// Since both wallets use shared storage, this works seamlessly.
///
/// # Arguments
/// * `sender` - The wallet that issued the contract
/// * `receiver` - The wallet that will receive contract information
/// * `contract_id` - The contract to share
/// * `key` - Storage key for the contract (e.g., "TestAsset1")
pub async fn share_contract(
    sender: &Arc<WalletController>,
    receiver: &Arc<WalletController>,
    contract_id: ContractId,
    key: &str,
) -> Result<()> {
    println!("  Sharing contract {} with key '{}'", contract_id, key);

    // Export contract to shared storage
    sender.export_contract(contract_id, key.to_string()).await?;

    // Import contract from shared storage
    receiver
        .import_contract(contract_id, key.to_string())
        .await?;

    // Sync receiver's runtime
    receiver.sync_runtime().await?;

    println!("  ✓ Contract shared successfully");
    Ok(())
}

/// Generate a fixed xpriv from a deterministic seed
///
/// Used for testing multisig where we need reproducible keys
pub fn generate_fixed_xpriv(name: &str) -> bitcoin::bip32::Xpriv {
    use bitcoin::hashes::{sha256, Hash};

    let seed_str = format!("test_seed_{}_do_not_use_in_production", name);
    let seed_hash = sha256::Hash::hash(seed_str.as_bytes());

    bitcoin::bip32::Xpriv::new_master(bitcoin::Network::Regtest, &seed_hash[..])
        .expect("Failed to create xpriv from seed")
}

/// Get TxOut from indexer for a given outpoint
///
/// # Arguments
/// * `outpoint` - The outpoint to query
/// * `instance` - Bitcoin node instance
/// * `network` - Network type
///
/// # Returns
/// TxOut information (value and script_pubkey)
pub fn get_txout_from_chain(
    outpoint: &bitcoin::OutPoint,
    instance: u8,
    network: Network,
) -> Result<bitcoin::TxOut> {
    let indexer_url = chain::indexer_url(instance, network);
    let client = esplora::Builder::new(&indexer_url)
        .build_blocking()
        .context("Failed to create esplora client")?;

    // Convert to bp outpoint
    let bp_outpoint = lightning_rgb_types::convert_outpoint_from_bitcoin_to_bp(*outpoint);

    // Get tx info from indexer
    let bp_esplora_tx = client
        .tx_info(&bp_outpoint.txid)
        .context("Failed to get transaction from indexer")?
        .ok_or_else(|| anyhow::anyhow!("Transaction not found"))?;

    // Get the output from vout list
    let bp_txout = bp_esplora_tx
        .vout
        .get(outpoint.vout as usize)
        .ok_or_else(|| anyhow::anyhow!("Output index {} not found", outpoint.vout))?;

    // Convert to bitcoin::TxOut
    let value = bitcoin::Amount::from_sat(bp_txout.value);
    let script_pubkey = bitcoin::ScriptBuf::from_bytes(bp_txout.scriptpubkey.to_vec());

    Ok(bitcoin::TxOut {
        value,
        script_pubkey,
    })
}

/// Sign transaction with xpriv (for single-sig P2WPKH inputs)
///
/// # Arguments
/// * `tx` - The unsigned transaction
/// * `xpriv` - The extended private key to sign with
/// * `input_utxos` - UTXOs being spent (value and script_pubkey)
///
/// # Returns
/// Signed transaction
pub fn sign_transaction_with_xpriv(
    mut tx: bitcoin::Transaction,
    xpriv: bitcoin::bip32::Xpriv,
    input_utxos: Vec<(bitcoin::TxOut, usize)>, // (TxOut, input_index)
) -> Result<bitcoin::Transaction> {
    use bitcoin::sighash::{SighashCache, EcdsaSighashType};
    use bitcoin::secp256k1::Secp256k1;

    let secp = Secp256k1::new();

    for (txout, input_idx) in input_utxos {
        // Create sighash
        let mut sighash_cache = SighashCache::new(&tx);
        let sighash = sighash_cache
            .p2wpkh_signature_hash(
                input_idx,
                &txout.script_pubkey,
                txout.value,
                EcdsaSighashType::All,
            )
            .context("Failed to compute sighash")?;

        // Sign with private key
        let msg = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
            .context("Failed to create message")?;
        let sig = secp.sign_ecdsa(&msg, &xpriv.private_key);
        let mut sig_bytes = sig.serialize_der().to_vec();
        sig_bytes.push(EcdsaSighashType::All as u8);

        // Get public key
        let pubkey = bitcoin::PublicKey::new(
            bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &xpriv.private_key)
        );

        // Assemble witness
        let witness = bitcoin::Witness::from_slice(&[
            sig_bytes,
            pubkey.to_bytes(),
        ]);

        // Set witness for input
        if let Some(input) = tx.input.get_mut(input_idx) {
            input.witness = witness;
        }
    }

    Ok(tx)
}

/// Sign transaction with wallet (using wallet's sign_finalize)
///
/// # Arguments
/// * `controller` - The wallet controller
/// * `tx` - The unsigned transaction
/// * `input_utxos` - UTXOs being spent (for witness_utxo in PSBT)
///
/// # Returns
/// Signed transaction
pub async fn sign_transaction(
    controller: &Arc<WalletController>,
    tx: bitcoin::Transaction,
    input_utxos: Vec<(bitcoin::TxOut, usize)>, // (TxOut, input_index)
) -> Result<bitcoin::Transaction> {
    // Convert Transaction to PSBT
    let mut bitcoin_psbt = bitcoin::Psbt::from_unsigned_tx(tx)
        .context("Failed to create PSBT from transaction")?;

    // Add witness_utxo for each input
    for (txout, input_idx) in input_utxos {
        if let Some(input) = bitcoin_psbt.inputs.get_mut(input_idx) {
            input.witness_utxo = Some(txout);
        }
    }

    // Convert to bp PSBT
    let bp_psbt = convert_psbt_from_bitcoin_to_bp(&bitcoin_psbt)
        .map_err(|e| anyhow::anyhow!("Failed to convert PSBT to bp format: {}", e))?;

    // Sign with wallet
    let signed_bp_psbt = controller.sign_finalize(bp_psbt).await;

    // Convert back to bitcoin PSBT
    let signed_bitcoin_psbt = convert_psbt_from_bp_to_bitcoin(&signed_bp_psbt);

    // Extract signed transaction
    let signed_tx = signed_bitcoin_psbt.extract_tx()
        .context("Failed to extract transaction from PSBT")?;

    Ok(signed_tx)
}

/// Broadcast transaction and mine blocks
///
/// # Arguments
/// * `tx` - The transaction to broadcast
/// * `blocks` - Number of blocks to mine after broadcasting
/// * `instance` - Bitcoin node instance to use
/// * `network` - Network type (for indexer URL)
///
/// # Returns
/// Transaction ID string
pub fn broadcast_and_mine(
    tx: &bitcoin::Transaction,
    blocks: u32,
    instance: u8,
    network: Network,
) -> Result<String> {
    // Get indexer URL and create esplora client
    let indexer_url = chain::indexer_url(instance, network);
    let client = esplora::Builder::new(&indexer_url)
        .build_blocking()
        .context("Failed to create esplora client")?;

    // Convert bitcoin::Transaction to bp::Tx for broadcast
    let bp_tx = convert_tx_from_bitcoin_to_bp(tx)
        .context("Failed to convert transaction to bp::Tx")?;

    // Broadcast transaction
    client
        .broadcast(&bp_tx)
        .context("Failed to broadcast transaction")?;

    let txid = tx.compute_txid().to_string();
    println!("  Broadcasted tx: {}", txid);

    // Mine blocks if requested
    if blocks > 0 {
        chain::mine_custom(true, instance, blocks);
        println!("  Mined {} blocks", blocks);
    }

    Ok(txid)
}

// ========== Lightning Commitment Transaction Helpers ==========

/// Create a commitment transaction spending from multisig
///
/// Creates a 2-output transaction splitting funds between Alice and Bob
///
/// # Arguments
/// * `multisig_outpoint` - The multisig UTXO to spend
/// * `alice_dest` - Alice's destination address (bitcoin::Address)
/// * `bob_dest` - Bob's destination address (bitcoin::Address)
/// * `alice_sats` - Satoshis to Alice
/// * `bob_sats` - Satoshis to Bob
///
/// # Returns
/// Unsigned commitment transaction
pub fn create_commitment_tx(
    multisig_outpoint: bitcoin::OutPoint,
    alice_dest: bitcoin::Address,
    bob_dest: bitcoin::Address,
    alice_sats: u64,
    bob_sats: u64,
) -> bitcoin::Transaction {
    use bitcoin::{Amount, ScriptBuf, Sequence, TxIn, TxOut, Witness};

    bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: multisig_outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::new(),
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(alice_sats),
                script_pubkey: alice_dest.script_pubkey(),
            },
            TxOut {
                value: Amount::from_sat(bob_sats),
                script_pubkey: bob_dest.script_pubkey(),
            },
        ],
    }
}

/// Create ColoringInfo for a commitment transaction
///
/// # Arguments
/// * `contract_id` - RGB contract ID
/// * `multisig_outpoint` - Input multisig UTXO
/// * `alice_rgb` - RGB amount to Alice (vout 0)
/// * `bob_rgb` - RGB amount to Bob (vout 1)
///
/// # Returns
/// ColoringInfo for the commitment transaction
pub fn create_coloring_info(
    contract_id: rgb::ContractId,
    multisig_outpoint: bitcoin::OutPoint,
    alice_rgb: u64,
    bob_rgb: u64,
) -> lightning_rgb_types::ColoringInfo {
    use lightning_rgb_ln_types::AssetId;
    use lightning_rgb_types::{
        contract_id_from_bytes, AssetColoringInfo, AssetIface, ColoringInfo, PlainOutpoint,
    };
    use std::collections::HashMap;

    ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_from_bytes(AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes())
                .unwrap(),
            AssetColoringInfo {
                iface: AssetIface::RGB20,
                input_outpoints: vec![PlainOutpoint {
                    txid: multisig_outpoint.txid.to_string(),
                    vout: multisig_outpoint.vout,
                }],
                output_map: HashMap::from([(0u32, alice_rgb), (1u32, bob_rgb)]),
                static_blinding: None,
            },
        )]),
        static_blinding: None,
        nonce: None,
    }
}

/// Color PSBT and verify Pending layer
///
/// # Arguments
/// * `wallet` - Wallet controller
/// * `tx` - Unsigned transaction
/// * `witness_utxo` - The UTXO being spent (for PSBT)
/// * `coloring_info` - RGB coloring information
/// * `contract_id` - RGB contract ID
/// * `party_name` - Name for logging (e.g., "Alice")
/// * `party_vout` - Output index (vout) for this party in the commitment tx
/// * `expected_rgb` - Expected RGB amount for this party (for verification)
/// * `expected_total_allocations` - Expected total number of allocations (default 2 for Alice+Bob, 3+ with HTLCs)
///
/// # Returns
/// (Colored PSBT, Colored txid)
pub async fn color_and_verify_pending(
    wallet: &Arc<WalletController>,
    tx: bitcoin::Transaction,
    witness_utxo: bitcoin::TxOut,
    coloring_info: lightning_rgb_types::ColoringInfo,
    contract_id: rgb::ContractId,
    party_name: &str,
    party_vout: u32,
    expected_rgb: u64,
    expected_total_allocations: Option<usize>,
) -> Result<(bitcoin::Psbt, bitcoin::Txid)> {
    use lightning_rgb_types::RgbStateLayer;

    // Create PSBT
    let mut psbt = bitcoin::Psbt::from_unsigned_tx(tx).context("Failed to create PSBT")?;
    psbt.inputs[0].witness_utxo = Some(witness_utxo);

    // Color PSBT
    let colored_psbt = wallet
        .color_psbt(psbt, coloring_info)
        .await
        .context("Failed to color PSBT")?;

    // IMPORTANT: Get txid AFTER coloring, as color_psbt modifies the transaction
    let colored_txid = colored_psbt.unsigned_tx.compute_txid();

    println!("{} colored commitment", party_name);

    // Verify Pending layer
    let pending = wallet
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Pending)
        .await
        .context("Failed to get Pending allocations")?;

    // Assert allocation count (default 2 for Alice+Bob, more with HTLCs)
    let expected_count = expected_total_allocations.unwrap_or(2);
    assert_eq!(
        pending.len(),
        expected_count,
        "{} Pending should have {} allocations, got {}",
        party_name,
        expected_count,
        pending.len()
    );

    // Find this party's allocation by (colored_txid, vout)
    let party_allocation = pending
        .iter()
        .find(|(outpoint, _)| {
            outpoint.txid.to_string() == colored_txid.to_string() && outpoint.vout.into_u32() == party_vout
        })
        .context(format!(
            "{} allocation not found in Pending for {}:{}",
            party_name, colored_txid, party_vout
        ))?;

    let actual_rgb = party_allocation.1;
    assert_eq!(
        actual_rgb, expected_rgb,
        "{} Pending RGB amount mismatch: expected {}, got {}",
        party_name, expected_rgb, actual_rgb
    );

    println!(
        "  {} Pending: {}:{} = {} RGB ✓",
        party_name, colored_txid, party_vout, actual_rgb
    );

    Ok((colored_psbt, colored_txid))
}

/// ACK commitment and verify Active layer
///
/// # Arguments
/// * `wallet` - Wallet controller
/// * `funding_txid` - Funding transaction ID (for ACK)
/// * `contract_id` - RGB contract ID
/// * `party_name` - Name for logging (e.g., "Alice")
/// * `commitment_txid` - The commitment transaction txid
/// * `party_vout` - Output index (vout) for this party in the commitment tx
/// * `expected_rgb` - Expected RGB amount for this party (for verification)
pub async fn ack_and_verify_active(
    wallet: &Arc<WalletController>,
    funding_txid: bitcoin::Txid,
    contract_id: rgb::ContractId,
    party_name: &str,
    commitment_txid: bitcoin::Txid,
    party_vout: u32,
    expected_rgb: u64,
) -> Result<()> {
    use lightning_rgb_types::RgbStateLayer;

    // ACK
    wallet
        .on_revoke_and_ack_by_commitment(funding_txid)
        .await
        .context("Failed to ACK")?;

    println!("{} ACK → Active", party_name);

    // Verify Active layer
    let active = wallet
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Active)
        .await
        .context("Failed to get Active allocations")?;

    // Assert allocation count
    assert_eq!(
        active.len(),
        2,
        "{} Active should have 2 allocations (Alice + Bob), got {}",
        party_name,
        active.len()
    );

    // Find this party's allocation by (commitment_txid, vout)
    let party_allocation = active
        .iter()
        .find(|(outpoint, _)| {
            outpoint.txid.to_string() == commitment_txid.to_string() && outpoint.vout.into_u32() == party_vout
        })
        .context(format!(
            "{} allocation not found in Active for {}:{}",
            party_name, commitment_txid, party_vout
        ))?;

    let actual_rgb = party_allocation.1;
    assert_eq!(
        actual_rgb, expected_rgb,
        "{} Active RGB amount mismatch: expected {}, got {}",
        party_name, expected_rgb, actual_rgb
    );

    println!(
        "  {} Active: {}:{} = {} RGB ✓\n",
        party_name, commitment_txid, party_vout, actual_rgb
    );

    Ok(())
}

/// Sign commitment transaction with multisig
///
/// # Arguments
/// * `tx` - Unsigned transaction
/// * `multisig` - MultiSig helper
/// * `multisig_script` - Multisig redeem script
/// * `input_value` - Input UTXO value
///
/// # Returns
/// Signed transaction
pub fn sign_commitment_tx(
    mut tx: bitcoin::Transaction,
    multisig: &super::multisig::MultiSigHelper,
    multisig_script: &bitcoin::ScriptBuf,
    input_value: bitcoin::Amount,
) -> Result<bitcoin::Transaction> {
    multisig
        .finalize_tx(&mut tx, multisig_script, input_value)
        .map_err(|e| anyhow::anyhow!("Failed to sign: {}", e))?;
    Ok(tx)
}

// ============================================================================
// Commitment Test Context - High-level API for commitment update testing
// ============================================================================

/// HTLC information for commitment testing
///
/// Minimal HTLC info needed for testing RGB coloring in commitment transactions.
/// Only tracks RGB amounts and direction, not time locks or payment details.
#[derive(Clone, Debug)]
pub struct HTLCInfo {
    /// Unique identifier for the HTLC (payment hash)
    pub payment_hash: [u8; 32],
    /// RGB amount in this HTLC
    pub amount_rgb: u64,
    /// Direction: true = Alice→Bob, false = Bob→Alice
    pub offered_by_alice: bool,
    /// Output index in commitment transaction (calculated at runtime)
    pub vout_in_commitment: Option<u32>,
}

/// Create minimal HTLC script for testing
///
/// Creates a simplified P2WSH script that only validates payment preimage.
/// This is much simpler than real LN HTLC scripts but sufficient for testing RGB coloring.
///
/// Script: OP_SHA256 <payment_hash> OP_EQUALVERIFY <pubkey> OP_CHECKSIG
/// Witness: <signature> <preimage> <witness_script>
pub fn create_minimal_htlc_script(
    payment_hash: &[u8; 32],
    recipient_pubkey: &bitcoin::secp256k1::PublicKey,
) -> bitcoin::ScriptBuf {
    use bitcoin::script::Builder;
    use bitcoin::opcodes;

    Builder::new()
        .push_opcode(opcodes::all::OP_SHA256)
        .push_slice(payment_hash)
        .push_opcode(opcodes::all::OP_EQUALVERIFY)
        .push_slice(&recipient_pubkey.serialize())
        .push_opcode(opcodes::all::OP_CHECKSIG)
        .into_script()
}

/// Create HTLC-Success transaction
///
/// Creates a transaction that spends an HTLC output (simulating successful preimage reveal)
///
/// # Arguments
/// * `htlc_outpoint` - The HTLC output from commitment TX
/// * `_htlc_amount` - Bitcoin amount in the HTLC output (unused, for documentation)
/// * `recipient_script` - Destination script pubkey for the funds
/// * `output_amount` - Amount to send (htlc_amount - fee)
///
/// # Returns
/// Unsigned HTLC-Success transaction
pub fn create_htlc_success_tx(
    htlc_outpoint: bitcoin::OutPoint,
    _htlc_amount: bitcoin::Amount,
    recipient_script: bitcoin::ScriptBuf,
    output_amount: bitcoin::Amount,
) -> bitcoin::Transaction {
    use bitcoin::{Sequence, Transaction, TxIn, TxOut, absolute::LockTime, transaction::Version};

    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: htlc_outpoint,
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![TxOut {
            value: output_amount,
            script_pubkey: recipient_script,
        }],
    }
}

/// Sign HTLC-Success transaction
///
/// Signs an HTLC-Success transaction with the recipient's key and adds the preimage.
/// The HTLC script expects witness: <signature> <preimage>
///
/// # Arguments
/// * `tx` - The HTLC-Success transaction to sign
/// * `htlc_script` - The HTLC witness script
/// * `htlc_amount` - Bitcoin amount in the HTLC output
/// * `payment_preimage` - The preimage (32 bytes)
/// * `recipient_xprv` - Recipient's xprv for signing
///
/// # Returns
/// Result indicating success or error
pub fn sign_htlc_success_tx(
    tx: &mut bitcoin::Transaction,
    htlc_script: &bitcoin::ScriptBuf,
    htlc_amount: bitcoin::Amount,
    payment_preimage: &[u8; 32],
    recipient_xprv: bitcoin::bip32::Xpriv,
) -> Result<()> {
    use bitcoin::sighash::{EcdsaSighashType, SighashCache};
    use bitcoin::secp256k1::Secp256k1;

    let secp = Secp256k1::new();

    // Create sighash for P2WSH spend
    let mut sighash_cache = SighashCache::new(tx.clone());
    let sighash = sighash_cache
        .p2wsh_signature_hash(
            0, // input_index
            htlc_script,
            htlc_amount,
            EcdsaSighashType::All,
        )
        .context("Failed to compute sighash")?;

    // Sign with master key (not derived)
    let private_key = &recipient_xprv.private_key;
    let msg = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
        .context("Failed to create message")?;
    let sig = secp.sign_ecdsa(&msg, private_key);
    let mut sig_bytes = sig.serialize_der().to_vec();
    sig_bytes.push(EcdsaSighashType::All as u8);

    // Assemble witness for HTLC-Success: <signature> <preimage> <htlc_script>
    let witness = bitcoin::Witness::from_slice(&[
        sig_bytes,
        payment_preimage.to_vec(),
        htlc_script.to_bytes(),
    ]);

    tx.input[0].witness = witness;

    Ok(())
}

/// Color HTLC transaction and verify Base layer update
///
/// Similar to color_and_verify_pending but for HTLC transactions.
/// Colors an HTLC-Success/Timeout transaction and verifies the RGB allocation.
///
/// # Arguments
/// * `wallet` - Wallet controller (recipient's wallet)
/// * `htlc_tx` - Unsigned HTLC transaction
/// * `htlc_witness_utxo` - The HTLC output being spent (for PSBT)
/// * `commitment_txid` - Parent commitment transaction ID
/// * `htlc_vout` - HTLC output index in commitment TX
/// * `htlc_rgb_amount` - RGB amount in the HTLC
/// * `contract_id` - RGB contract ID
/// * `party_name` - Name for logging
/// * `expected_rgb` - Expected RGB amount for verification
///
/// # Returns
/// (Colored PSBT, Colored HTLC txid)
pub async fn color_htlc_tx(
    wallet: &Arc<WalletController>,
    htlc_tx: bitcoin::Transaction,
    htlc_witness_utxo: bitcoin::TxOut,
    commitment_txid: bitcoin::Txid,
    htlc_vout: u32,
    htlc_rgb_amount: u64,
    contract_id: rgb::ContractId,
    party_name: &str,
    expected_rgb: u64,
) -> Result<(bitcoin::Psbt, bitcoin::Txid)> {
    use lightning_rgb_types::RgbStateLayer;

    // Create ColoringInfo for HTLC TX
    let coloring_info = lightning_rgb_types::ColoringInfo {
        asset_info_map: std::collections::HashMap::from_iter([(
            contract_id_from_bytes(AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes())
                .unwrap(),
            lightning_rgb_types::AssetColoringInfo {
                iface: lightning_rgb_types::AssetIface::RGB20,
                input_outpoints: vec![lightning_rgb_types::PlainOutpoint {
                    txid: commitment_txid.to_string(),
                    vout: htlc_vout,
                }],
                output_map: std::collections::HashMap::from([(0u32, htlc_rgb_amount)]),
                static_blinding: None,
            },
        )]),
        static_blinding: None,
        nonce: Some(1),
    };

    // Create PSBT
    let mut psbt = bitcoin::Psbt::from_unsigned_tx(htlc_tx).context("Failed to create PSBT")?;
    psbt.inputs[0].witness_utxo = Some(htlc_witness_utxo);

    // Color PSBT
    let colored_psbt = wallet
        .color_psbt(psbt, coloring_info)
        .await
        .context("Failed to color HTLC PSBT")?;

    let colored_txid = colored_psbt.unsigned_tx.compute_txid();

    println!("{} colored HTLC-Success TX", party_name);

    // Verify Pending layer (HTLC-Success TX is colored but not yet on-chain)
    let pending = wallet
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Pending)
        .await
        .context("Failed to get Pending allocations")?;

    // Find recipient's allocation
    let recipient_allocation = pending
        .iter()
        .find(|(outpoint, _)| {
            outpoint.txid.to_string() == colored_txid.to_string() && outpoint.vout.into_u32() == 0
        })
        .context(format!(
            "{} allocation not found in Pending for HTLC TX {}:0",
            party_name, colored_txid
        ))?;

    let actual_rgb = recipient_allocation.1;
    assert_eq!(
        actual_rgb, expected_rgb,
        "{} Pending RGB amount mismatch: expected {}, got {}",
        party_name, expected_rgb, actual_rgb
    );

    println!(
        "  {} Pending: {}:0 = {} RGB ✓",
        party_name, colored_txid, actual_rgb
    );

    Ok((colored_psbt, colored_txid))
}

/// Commitment test context
///
/// Encapsulates all state and parameters needed for commitment testing,
/// providing a high-level API for testing commitment updates.
pub struct CommitmentTestContext {
    alice: Arc<WalletController>,
    bob: Arc<WalletController>,
    multisig_outpoint: bitcoin::OutPoint,
    multisig_txout: bitcoin::TxOut,
    multisig: super::multisig::MultiSigHelper,
    multisig_script: bitcoin::ScriptBuf,
    alice_dest: bitcoin::Address,
    bob_dest: bitcoin::Address,
    alice_sats: u64,
    bob_sats: u64,
    contract_id: rgb::ContractId,
    round: std::cell::Cell<usize>,
}

impl CommitmentTestContext {
    /// Create a new builder
    pub fn builder() -> CommitmentTestContextBuilder {
        CommitmentTestContextBuilder::default()
    }

    /// Get reference to multisig helper
    pub fn multisig(&self) -> &super::multisig::MultiSigHelper {
        &self.multisig
    }

    /// Get Arc-wrapped multisig helper (for sweep TX signing)
    pub fn multisig_arc(&self) -> std::sync::Arc<super::multisig::MultiSigHelper> {
        std::sync::Arc::new(self.multisig.clone())
    }

    /// Get reference to multisig script
    pub fn multisig_script(&self) -> &bitcoin::ScriptBuf {
        &self.multisig_script
    }

    /// Set RGB allocation (returns a new round builder)
    pub fn with_rgb(&self, alice_rgb: u64, bob_rgb: u64) -> CommitmentRound {
        CommitmentRound {
            ctx: self,
            alice_rgb,
            bob_rgb,
            htlcs: Vec::new(),
        }
    }
}

/// Builder for CommitmentTestContext
#[derive(Default)]
pub struct CommitmentTestContextBuilder {
    alice: Option<Arc<WalletController>>,
    bob: Option<Arc<WalletController>>,
    multisig_outpoint: Option<bitcoin::OutPoint>,
    multisig_txout: Option<bitcoin::TxOut>,
    multisig: Option<super::multisig::MultiSigHelper>,
    multisig_script: Option<bitcoin::ScriptBuf>,
    alice_dest: Option<bitcoin::Address>,
    bob_dest: Option<bitcoin::Address>,
    alice_sats: Option<u64>,
    bob_sats: Option<u64>,
    contract_id: Option<rgb::ContractId>,
}

impl CommitmentTestContextBuilder {
    /// Set wallets for both parties
    pub fn with_wallets(mut self, alice: Arc<WalletController>, bob: Arc<WalletController>) -> Self {
        self.alice = Some(alice);
        self.bob = Some(bob);
        self
    }

    /// Set multisig parameters
    pub fn with_multisig(
        mut self,
        outpoint: bitcoin::OutPoint,
        txout: bitcoin::TxOut,
        multisig: super::multisig::MultiSigHelper,
        script: bitcoin::ScriptBuf,
    ) -> Self {
        self.multisig_outpoint = Some(outpoint);
        self.multisig_txout = Some(txout);
        self.multisig = Some(multisig);
        self.multisig_script = Some(script);
        self
    }

    /// Set destination addresses for both parties
    pub fn with_addresses(mut self, alice_dest: bitcoin::Address, bob_dest: bitcoin::Address) -> Self {
        self.alice_dest = Some(alice_dest);
        self.bob_dest = Some(bob_dest);
        self
    }

    /// Set BTC allocation for both parties
    pub fn with_btc_allocation(mut self, alice_sats: u64, bob_sats: u64) -> Self {
        self.alice_sats = Some(alice_sats);
        self.bob_sats = Some(bob_sats);
        self
    }

    /// Set RGB contract ID
    pub fn with_contract(mut self, contract_id: rgb::ContractId) -> Self {
        self.contract_id = Some(contract_id);
        self
    }

    /// Build the context (validates all required fields are set)
    pub fn build(self) -> Result<CommitmentTestContext> {
        Ok(CommitmentTestContext {
            alice: self.alice.context("Missing alice wallet")?,
            bob: self.bob.context("Missing bob wallet")?,
            multisig_outpoint: self.multisig_outpoint.context("Missing multisig_outpoint")?,
            multisig_txout: self.multisig_txout.context("Missing multisig_txout")?,
            multisig: self.multisig.context("Missing multisig")?,
            multisig_script: self.multisig_script.context("Missing multisig_script")?,
            alice_dest: self.alice_dest.context("Missing alice_dest")?,
            bob_dest: self.bob_dest.context("Missing bob_dest")?,
            alice_sats: self.alice_sats.context("Missing alice_sats")?,
            bob_sats: self.bob_sats.context("Missing bob_sats")?,
            contract_id: self.contract_id.context("Missing contract_id")?,
            round: std::cell::Cell::new(0),
        })
    }
}

/// Single round of commitment update
///
/// Separates concerns: coloring and ACK are decoupled operations.
/// You can color to Pending layer first, then optionally ACK to Active layer.
pub struct CommitmentRound<'a> {
    ctx: &'a CommitmentTestContext,
    alice_rgb: u64,
    bob_rgb: u64,
    htlcs: Vec<HTLCInfo>,  // HTLC list for this commitment
}

impl<'a> CommitmentRound<'a> {
    /// Add an HTLC to this commitment
    ///
    /// # Arguments
    /// * `payment_hash` - Unique identifier for the HTLC
    /// * `amount_rgb` - RGB amount in this HTLC
    /// * `offered_by_alice` - Direction: true = Alice→Bob, false = Bob→Alice
    ///
    /// # Returns
    /// Self for chaining
    pub fn add_htlc(
        mut self,
        payment_hash: [u8; 32],
        amount_rgb: u64,
        offered_by_alice: bool,
    ) -> Self {
        self.htlcs.push(HTLCInfo {
            payment_hash,
            amount_rgb,
            offered_by_alice,
            vout_in_commitment: None,
        });
        self
    }

    /// Color PSBT to Pending layer only (does not ACK)
    ///
    /// Returns a PendingCommitment that can be optionally ACKed later
    pub async fn color_pending(self) -> Result<PendingCommitment<'a>> {
        let round = self.ctx.round.get() + 1;
        self.ctx.round.set(round);

        println!(
            "=== Commitment #{} (Alice {}k / Bob {}k) - Pending ===\n",
            round,
            self.alice_rgb / 1000,
            self.bob_rgb / 1000
        );

        // Calculate RGB balances after HTLC deductions
        let mut alice_remaining_rgb = self.alice_rgb;
        let mut bob_remaining_rgb = self.bob_rgb;

        for htlc in &self.htlcs {
            if htlc.offered_by_alice {
                alice_remaining_rgb = alice_remaining_rgb.checked_sub(htlc.amount_rgb)
                    .context("Alice RGB balance underflow from HTLC")?;
            } else {
                bob_remaining_rgb = bob_remaining_rgb.checked_sub(htlc.amount_rgb)
                    .context("Bob RGB balance underflow from HTLC")?;
            }
        }

        // Create commitment transaction with HTLC outputs
        let mut commitment_tx = create_commitment_tx(
            self.ctx.multisig_outpoint,
            self.ctx.alice_dest.clone(),
            self.ctx.bob_dest.clone(),
            self.ctx.alice_sats,
            self.ctx.bob_sats,
        );

        // Add HTLC outputs to commitment transaction
        let mut htlc_infos = self.htlcs.clone();
        for (idx, htlc) in htlc_infos.iter_mut().enumerate() {
            // Determine recipient (who receives the HTLC output)
            let recipient_pubkey = if htlc.offered_by_alice {
                self.ctx.multisig.derive_bob_pubkey()  // Alice→Bob, Bob receives
            } else {
                self.ctx.multisig.derive_alice_pubkey()  // Bob→Alice, Alice receives
            };

            // Create minimal HTLC script
            let htlc_script = create_minimal_htlc_script(&htlc.payment_hash, &recipient_pubkey);

            // Add HTLC output (fixed 5000 sats for BTC amount)
            // Create P2WSH script pubkey from HTLC script
            commitment_tx.output.push(bitcoin::TxOut {
                value: bitcoin::Amount::from_sat(5000),
                script_pubkey: htlc_script.to_p2wsh(),
            });

            // Record vout in commitment TX (0=Alice, 1=Bob, 2+=HTLCs)
            htlc.vout_in_commitment = Some(2 + idx as u32);
        }

        // Build output_map for RGB allocation
        // vout 0: Alice (remaining RGB after HTLCs)
        // vout 1: Bob (remaining RGB after HTLCs)
        // vout 2+: HTLC outputs
        let mut output_map = std::collections::HashMap::new();
        output_map.insert(0u32, alice_remaining_rgb);
        output_map.insert(1u32, bob_remaining_rgb);

        // Add HTLC RGB allocations
        for htlc in &htlc_infos {
            if let Some(vout) = htlc.vout_in_commitment {
                output_map.insert(vout, htlc.amount_rgb);
            }
        }

        // Create ColoringInfo with complete output_map (including HTLCs)
        use lightning_rgb_ln_types::AssetId;
        use lightning_rgb_types::{
            contract_id_from_bytes, AssetColoringInfo, AssetIface, ColoringInfo, PlainOutpoint,
        };

        let coloring_info = ColoringInfo {
            asset_info_map: std::collections::HashMap::from_iter([(
                contract_id_from_bytes(AssetId::from(self.ctx.contract_id.to_byte_array().as_slice()).as_bytes())
                    .unwrap(),
                AssetColoringInfo {
                    iface: AssetIface::RGB20,
                    input_outpoints: vec![PlainOutpoint {
                        txid: self.ctx.multisig_outpoint.txid.to_string(),
                        vout: self.ctx.multisig_outpoint.vout,
                    }],
                    output_map,
                    static_blinding: None,
                },
            )]),
            static_blinding: None,
            nonce: None,
        };

        // Calculate expected total allocations (2 for Alice+Bob + number of HTLCs)
        let expected_total_allocations = 2 + self.htlcs.len();

        // Color Alice's commitment (verify remaining RGB after HTLC deductions)
        let (alice_colored_psbt, alice_txid) = color_and_verify_pending(
            &self.ctx.alice,
            commitment_tx.clone(),
            self.ctx.multisig_txout.clone(),
            coloring_info.clone(),
            self.ctx.contract_id,
            "Alice",
            0,
            alice_remaining_rgb,  // Verify remaining RGB (after HTLC deductions)
            Some(expected_total_allocations),
        )
        .await
        .context("Failed Alice color")?;

        // Color Bob's commitment (verify remaining RGB after HTLC deductions)
        let (_, bob_txid) = color_and_verify_pending(
            &self.ctx.bob,
            commitment_tx,
            self.ctx.multisig_txout.clone(),
            coloring_info,
            self.ctx.contract_id,
            "Bob",
            1,
            bob_remaining_rgb,  // Verify remaining RGB (after HTLC deductions)
            Some(expected_total_allocations),
        )
        .await
        .context("Failed Bob color")?;

        // Extract the colored commitment TX from Alice's PSBT
        // NOTE: In real LN, Alice and Bob have different commitment TXs (different scripts).
        // In our test simulation, we temporarily use the same TX structure for simplicity,
        // so we can use either Alice's or Bob's colored result.
        let colored_commitment_tx = alice_colored_psbt.unsigned_tx.clone();

        Ok(PendingCommitment {
            ctx: self.ctx,
            alice_txid,
            bob_txid,
            alice_rgb: self.alice_rgb,
            bob_rgb: self.bob_rgb,
            commitment_tx: colored_commitment_tx,  // Store the colored TX
            htlcs: htlc_infos,  // Store HTLC information (with vout_in_commitment set)
        })
    }

    /// Color PSBT and immediately ACK to Active layer (convenience method)
    pub async fn color_and_ack(self) -> Result<()> {
        self.color_pending().await?.ack().await
    }
}

/// Pending commitment that can be optionally ACKed
///
/// Represents a commitment that has been colored to Pending layer
/// but not yet ACKed to Active layer. You can choose to ACK it later.
pub struct PendingCommitment<'a> {
    ctx: &'a CommitmentTestContext,
    alice_txid: bitcoin::Txid,
    bob_txid: bitcoin::Txid,
    alice_rgb: u64,
    bob_rgb: u64,
    commitment_tx: bitcoin::Transaction,  // Colored commitment TX (can be retrieved multiple times)
    htlcs: Vec<HTLCInfo>,  // HTLC information for this commitment
}

impl<'a> PendingCommitment<'a> {
    /// Get the colored commitment TX
    ///
    /// Returns the commitment transaction that has been colored with RGB state.
    /// This TX can be broadcast (after signing) for force close scenarios.
    pub fn commitment_tx(&self) -> &bitcoin::Transaction {
        &self.commitment_tx
    }

    /// Get HTLC information for this commitment
    ///
    /// Returns the list of HTLCs included in this commitment transaction.
    pub fn htlcs(&self) -> &[HTLCInfo] {
        &self.htlcs
    }

    /// ACK this commitment to Active layer
    ///
    /// Moves the commitment from Pending to Active state
    pub async fn ack(self) -> Result<()> {
        ack_and_verify_active(
            &self.ctx.alice,
            self.ctx.multisig_outpoint.txid,
            self.ctx.contract_id,
            "Alice",
            self.alice_txid,
            0,
            self.alice_rgb,
        )
        .await
        .context("Failed Alice ACK")?;

        ack_and_verify_active(
            &self.ctx.bob,
            self.ctx.multisig_outpoint.txid,
            self.ctx.contract_id,
            "Bob",
            self.bob_txid,
            1,
            self.bob_rgb,
        )
        .await
        .context("Failed Bob ACK")?;

        Ok(())
    }
}

/// Parameters for creating a sweep transaction
pub struct SweepTxParams {
    /// Input to sweep
    pub input: bitcoin::OutPoint,
    /// Witness UTXO from previous TX
    pub witness_utxo: bitcoin::TxOut,
    /// Destination address
    pub dest_address: bpstd::Address,
    /// Bitcoin amount in input
    pub input_amount_sats: u64,
    /// Sweep fee
    pub fee_sats: u64,
    /// Contract ID
    pub contract_id: rgb::ContractId,
    /// RGB amount to transfer
    pub rgb_amount: u64,
    /// Signer xpriv
    pub signer_xpriv: bitcoin::bip32::Xpriv,
    /// Multisig helper for signing (uses master key, no derivation)
    pub multisig: std::sync::Arc<super::multisig::MultiSigHelper>,
}

/// Create, color, sign, finalize and broadcast a sweep transaction
///
/// This function encapsulates the complete sweep TX workflow:
/// 1. Create unsigned TX (input → output)
/// 2. Build ColoringInfo
/// 3. Convert to PSBT + add witness_utxo
/// 4. color_psbt() (will detect is_sweep_tx=true via applied overlays)
/// 5. Sign with P2WPKH key
/// 6. finalize_rgb_tx() (apply to runtime before broadcast)
/// 7. broadcast_and_mine()
///
/// # Arguments
/// * `wallet` - Wallet controller
/// * `params` - Sweep TX parameters
/// * `party_name` - Name for logging (e.g., "Alice", "Bob HTLC")
/// * `instance` - Bitcoin node instance
/// * `network` - Network type
///
/// # Returns
/// The broadcast txid string
pub async fn create_and_broadcast_sweep_tx(
    wallet: &Arc<WalletController>,
    params: SweepTxParams,
    party_name: &str,
    instance: u8,
    network: Network,
) -> Result<String> {
    use lightning_rgb_types::{AssetColoringInfo, AssetIface, PlainOutpoint};
    use std::collections::HashMap;

    println!("  Creating {} sweep TX...", party_name);

    let output_amount = params.input_amount_sats - params.fee_sats;

    // 1. Create unsigned TX
    let sweep_tx = bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: params.input,
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![bitcoin::TxOut {
            value: bitcoin::Amount::from_sat(output_amount),
            script_pubkey: bitcoin::ScriptBuf::from_bytes(params.dest_address.script_pubkey().to_vec()),
        }],
    };

    // 2. Build ColoringInfo
    let contract_id_converted = contract_id_from_bytes(
        AssetId::from(params.contract_id.to_byte_array().as_slice()).as_bytes(),
    )
    .map_err(|e| anyhow::anyhow!("Failed to convert contract_id: {}", e))?;

    let coloring = lightning_rgb_types::ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_converted,
            AssetColoringInfo {
                iface: AssetIface::RGB20,
                input_outpoints: vec![PlainOutpoint {
                    txid: params.input.txid.to_string(),
                    vout: params.input.vout,
                }],
                output_map: HashMap::from([(0u32, params.rgb_amount)]),
                static_blinding: None,
            },
        )]),
        static_blinding: None,
        nonce: None,
    };

    // 3. Create PSBT with witness
    let sweep_psbt = bitcoin::Psbt::from_unsigned_tx(sweep_tx.clone())
        .context("Failed to create PSBT")?;
    let mut sweep_psbt_with_witness = sweep_psbt.clone();
    sweep_psbt_with_witness.inputs[0].witness_utxo = Some(params.witness_utxo);

    // 4. Color PSBT (will detect is_sweep_tx=true)
    let colored_psbt = wallet
        .color_psbt(sweep_psbt_with_witness, coloring)
        .await
        .context(format!("Failed to color {} sweep PSBT", party_name))?;

    println!("  ✓ {} sweep TX colored (is_sweep detected)", party_name);

    // 5. Sign TX (P2WPKH)
    let mut sweep_tx_final = colored_psbt.unsigned_tx.clone();
    params.multisig
        .sign_p2wpkh_tx(
            &mut sweep_tx_final,
            &params.signer_xpriv,
            bitcoin::Amount::from_sat(params.input_amount_sats),
        )
        .map_err(|e| anyhow::anyhow!("Failed to sign {} sweep TX: {}", party_name, e))?;

    println!("  ✓ {} sweep TX signed", party_name);

    // 6. Finalize RGB state
    wallet
        .finalize_rgb_tx(sweep_tx_final.clone())
        .await
        .context(format!("Failed to finalize {} RGB TX", party_name))?;

    // 7. Broadcast
    let sweep_txid = broadcast_and_mine(&sweep_tx_final, 1, instance, network)
        .context(format!("Failed to broadcast {} sweep TX", party_name))?;

    println!("  ✓ {} sweep TX confirmed: {}", party_name, sweep_txid);

    Ok(sweep_txid)
}

// =============================================================================
// High-Level Test Scenario Helpers
// =============================================================================

/// Complete channel setup result with all necessary components
pub struct ChannelSetup {
    pub alice: Arc<WalletController>,
    pub bob: Arc<WalletController>,
    pub contract_id: ContractId,
    pub multisig_outpoint: bitcoin::OutPoint,
    pub multisig_txout: bitcoin::TxOut,
    pub multisig: Arc<MultiSigHelper>,
    pub network: bpstd::Network,
}

/// Setup a complete Lightning channel with RGB funding
///
/// This encapsulates the common "Phase 1" setup across all tests:
/// 1. Initialize chain and create wallets
/// 2. Issue RGB contract
/// 3. Share contract with counterparty
/// 4. Create multisig funding TX with RGB
/// 5. Broadcast and confirm funding TX
///
/// # Arguments
/// * `test_prefix` - Unique prefix for wallet names (e.g., "coop_close")
/// * `rgb_amount` - Total RGB amount to fund the channel
/// * `contract_name` - Contract name (e.g., "TestNIA")
/// * `contract_ticker` - Contract ticker (e.g., "TNIA")
///
/// # Returns
/// ChannelSetup with all components ready for commitment creation
pub async fn setup_channel_with_rgb(
    test_prefix: &str,
    rgb_amount: u64,
    contract_name: &str,
    contract_ticker: &str,
) -> Result<ChannelSetup> {
    // 1. Initialize chain and wallets
    chain::initialize().await;
    let network = bpstd::Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    let alice_xprv = generate_fixed_xpriv(&format!("{}_alice", test_prefix));
    let bob_xprv = generate_fixed_xpriv(&format!("{}_bob", test_prefix));

    let alice = crate::utils::wallet::create_wallet_with_xprv(
        &format!("{}_alice", test_prefix),
        network,
        indexer_url.clone(),
        alice_xprv,
    )
    .await
    .context("Failed to create Alice wallet")?;

    let bob = crate::utils::wallet::create_wallet_with_xprv(
        &format!("{}_bob", test_prefix),
        network,
        indexer_url,
        bob_xprv,
    )
    .await
    .context("Failed to create Bob wallet")?;

    println!("  ✓ Wallets created: {}_alice, {}_bob", test_prefix, test_prefix);

    // 2. Issue RGB contract
    let issue_utxo = get_utxo(&alice, Some(200_000), INSTANCE_1)
        .await
        .context("Failed to get issue UTXO")?;

    let mut params = NIAIssueParams::new(contract_name, contract_ticker, "centiMilli", rgb_amount);
    params.add_allocation(issue_utxo, rgb_amount);

    let contract_id = alice
        .issue(params.into_create_params())
        .await
        .context("Failed to issue contract")?;

    println!("  ✓ Issued contract: {}", contract_id);

    // 3. Share with Bob
    share_contract(&alice, &bob, contract_id, contract_name)
        .await
        .context("Failed to share contract")?;

    println!("  ✓ Contract shared with Bob");

    // 4. Create multisig funding TX
    let multisig = MultiSigHelper::new(alice_xprv, bob_xprv);
    let multisig_addr = multisig.create_multisig_address(bitcoin::Network::Regtest);

    let channel_value_sats = 100_000;

    let request = RgbFundingRequest {
        output_script: multisig_addr.script_pubkey(),
        channel_value_sats,
        asset_quantity: AssetQuantity {
            asset_id: AssetId::from(contract_id.to_byte_array().as_slice()),
            amount: rgb_amount,
        },
    };

    let funding_result = alice
        .create_funding_psbt(request, 1.0, None)
        .await
        .context("Failed to create funding PSBT")?;

    let bp_psbt = convert_psbt_from_bitcoin_to_bp(&funding_result.psbt)
        .map_err(|e| anyhow::anyhow!("Failed to convert PSBT: {}", e))?;

    let signed_bp_psbt = alice.sign_finalize(bp_psbt).await;
    let signed_bitcoin_psbt = convert_psbt_from_bp_to_bitcoin(&signed_bp_psbt);
    let signed_funding_tx = signed_bitcoin_psbt
        .extract_tx()
        .context("Failed to extract TX")?;

    // 5. Broadcast and confirm
    let funding_txid = broadcast_and_mine(&signed_funding_tx, 1, INSTANCE_1, network)
        .context("Failed to broadcast funding TX")?;

    bob.accept_transfer(funding_txid.clone())
        .await
        .context("Failed Bob accept transfer")?;

    alice.sync_runtime().await.context("Failed sync Alice")?;
    bob.sync_runtime().await.context("Failed sync Bob")?;

    // 6. Find multisig outpoint
    let multisig_script_pubkey = multisig_addr.script_pubkey();
    let multisig_vout = signed_funding_tx
        .output
        .iter()
        .position(|o| o.script_pubkey == multisig_script_pubkey)
        .context("Multisig output not found")?;

    let multisig_outpoint = bitcoin::OutPoint {
        txid: bitcoin::Txid::from_str(&funding_txid).context("Failed to parse txid")?,
        vout: multisig_vout as u32,
    };

    let multisig_txout = signed_funding_tx.output[multisig_vout].clone();

    println!("  ✓ Multisig UTXO: {}:{}", multisig_outpoint.txid, multisig_outpoint.vout);

    Ok(ChannelSetup {
        alice,
        bob,
        contract_id,
        multisig_outpoint,
        multisig_txout,
        multisig: Arc::new(multisig),
        network,
    })
}

/// Sweep a single output to wallet with RGB
///
/// Encapsulates the common sweep pattern:
/// 1. Get destination address
/// 2. Create SweepTxParams
/// 3. Call create_and_broadcast_sweep_tx
///
/// # Arguments
/// * `wallet` - The wallet performing the sweep
/// * `input_tx` - The transaction containing the output to sweep
/// * `vout` - The output index to sweep
/// * `rgb_amount` - RGB amount in this output
/// * `party_name` - Name for logging (e.g., "Alice", "Bob")
/// * `label` - Description for logging (e.g., "commitment", "HTLC")
/// * `ctx` - Commitment test context with multisig info
/// * `signer_xpriv` - The xpriv to sign this sweep TX
///
/// # Returns
/// The sweep transaction ID
pub async fn sweep_output_to_wallet(
    wallet: &Arc<WalletController>,
    input_tx: &bitcoin::Transaction,
    vout: u32,
    rgb_amount: u64,
    party_name: &str,
    label: &str,
    ctx: &CommitmentTestContext,
    signer_xpriv: &bitcoin::bip32::Xpriv,
) -> Result<String> {

    let input_txid = input_tx.compute_txid();
    let witness_utxo = input_tx.output[vout as usize].clone();
    let input_amount_sats = witness_utxo.value.to_sat();

    // Get destination address
    let dest_address = wallet
        .get_address()
        .await
        .context(format!("Failed to get {} destination address", party_name))?;

    // Create sweep params
    let params = SweepTxParams {
        input: bitcoin::OutPoint {
            txid: input_txid,
            vout,
        },
        witness_utxo,
        dest_address,
        input_amount_sats,
        fee_sats: 500,
        contract_id: ctx.contract_id,
        rgb_amount,
        signer_xpriv: signer_xpriv.clone(),
        multisig: ctx.multisig_arc(),
    };

    // Perform sweep (use Regtest as default network for test context)
    let sweep_txid = create_and_broadcast_sweep_tx(
        wallet,
        params,
        &format!("{} {}", party_name, label),
        INSTANCE_1,
        bpstd::Network::Regtest,
    )
    .await
    .context(format!("Failed to sweep {} {}", party_name, label))?;

    println!("  ✓ {} swept {} RGB from {}", party_name, rgb_amount, label);

    Ok(sweep_txid)
}

/// Verify final RGB balances for both parties
///
/// Encapsulates the common balance verification:
/// 1. Sync both wallets
/// 2. Get allocations
/// 3. Sum totals
/// 4. Assert expected values
/// 5. Assert conservation
///
/// # Arguments
/// * `alice` - Alice's wallet
/// * `bob` - Bob's wallet
/// * `contract_id` - The contract to check
/// * `expected_alice` - Expected Alice total
/// * `expected_bob` - Expected Bob total
pub async fn verify_final_balances(
    alice: &Arc<WalletController>,
    bob: &Arc<WalletController>,
    contract_id: ContractId,
    expected_alice: u64,
    expected_bob: u64,
) -> Result<()> {
    // Sync wallets
    alice.sync_runtime().await.context("Failed to sync Alice")?;
    bob.sync_runtime().await.context("Failed to sync Bob")?;

    // Get allocations
    let alice_allocs = alice
        .get_allocations(contract_id)
        .await
        .context("Failed to get Alice allocations")?;
    let bob_allocs = bob
        .get_allocations(contract_id)
        .await
        .context("Failed to get Bob allocations")?;

    // Sum totals
    let alice_total: u64 = alice_allocs.iter().map(|a| a.1).sum();
    let bob_total: u64 = bob_allocs.iter().map(|a| a.1).sum();
    let total = alice_total + bob_total;

    println!("  Alice final balance: {} RGB", alice_total);
    println!("  Bob final balance: {} RGB", bob_total);

    // Assertions
    assert_eq!(
        alice_total, expected_alice,
        "Alice balance mismatch: expected {}, got {}",
        expected_alice, alice_total
    );
    assert_eq!(
        bob_total, expected_bob,
        "Bob balance mismatch: expected {}, got {}",
        expected_bob, bob_total
    );

    let expected_total = expected_alice + expected_bob;
    assert_eq!(
        total, expected_total,
        "RGB conservation failed: expected {}, got {}",
        expected_total, total
    );

    println!("  ✓ RGB conservation verified: {} + {} = {}", alice_total, bob_total, total);

    Ok(())
}
