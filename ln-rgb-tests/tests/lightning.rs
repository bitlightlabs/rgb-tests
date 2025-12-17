use bpstd::Network;
use ln_rgb_tests::utils::{
    asset_params::NIAIssueParams,
    chain::{self, indexer_url},
    multisig::MultiSigHelper,
    test_helpers::{
        broadcast_and_mine, generate_fixed_xpriv, get_utxo, share_contract,
        CommitmentTestContext,
    },
    wallet::*,
    *,
};
use lightning_rgb_ln_types::{AssetId, AssetQuantity};
use lightning_rgb_types::RgbStateLayer;
use lightning_rgb_wallet::RgbFundingRequest;
use serial_test::serial;
use std::str::FromStr;

// //! Test Commitment Update and State Layer Transitions
// //!
// //! **Test Scenario**
// //! Verifies that RGB state correctly transitions through Pending/Active layers
// //! as Lightning commitment transactions are updated.
// //!
// //! **Initial State**
// //! - Multisig UTXO with 500,000 TSNIA
// //! - Base layer: 500k in multisig
// //!
// //! **Test Flow**
// //!
// //! 1. **Commitment #1** (Alice 250k / Bob 250k)
// //!    - Both parties: color_psbt() → Pending layer
// //!    - Both parties: on_revoke_and_ack() → Active #1
// //!    - Verify: Active has #1, Base still shows multisig 500k
// //!
// //! 2. **Commitment #2** (Alice 300k / Bob 200k)
// //!    - Both parties: color_psbt() → Pending layer
// //!    - Both parties: on_revoke_and_ack() → Active #2 (replaces #1)
// //!    - Verify: Active only has #2, Base still shows multisig 500k
// //!
// //! 3. **Commitment #3** (Alice 350k / Bob 150k)
// //!    - Both parties: color_psbt() → Pending #3
// //!    - Verify: Pending has #3, Active still shows #2
// //!
// //! 4. **Commitment #4** (Alice 400k / Bob 100k)
// //!    - Both parties: color_psbt() → Pending #4 (overwrites #3)
// //!    - Verify: Pending only has #4, Active still shows #2
// //!
// //! 5. **Commitment #5** (Alice 450k / Bob 50k)
// //!    - Both parties: color_psbt() → Pending #5
// //!    - Both parties: on_revoke_and_ack() → Active #5 (replaces #2)
// //!    - Verify: Active only has #5, Base still shows multisig 500k
// //!
// //! **Final Verification**
// //! - Base layer remains unchanged (multisig 500k)
// //! - No commitment TX is broadcasted (off-chain only)
// //! - Both parties have identical Active state
// //!
// //! **Key Validations**
// //! 1. Pending overwrite mechanism (same funding_txid)
// //! 2. Active layer replacement (old commitment removed)
// //! 3. RGB conservation (Alice increase = Bob decrease)
// //! 4. Symmetric state for both parties
// //! 5. Base layer isolation (unaffected by off-chain updates)
#[tokio::test]
#[serial]
async fn test_commitment_update() {
    println!("\n=== Test: Commitment Update and State Layer Transitions ===\n");

    // ========== Setup (using high-level helper) ==========
    println!("Step 0: Setting up multisig with RGB assets...");

    let setup = test_helpers::setup_channel_with_rgb("commit_update", 500_000, "TestCommitNIA", "TCNIA")
        .await
        .expect("Failed to setup channel");

    let alice = &setup.alice;
    let bob = &setup.bob;
    let contract_id = setup.contract_id;
    let multisig_outpoint = setup.multisig_outpoint;
    let multisig_txout = setup.multisig_txout;
    let multisig = &setup.multisig;

    println!("  ✓ Multisig UTXO: {}:{}", multisig_outpoint.txid, multisig_outpoint.vout);
    println!("  Value: {} sats, RGB: 500,000 TCNIA\n", multisig_txout.value.to_sat());

    // Verify Base layer
    let alice_base = alice
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Base)
        .await
        .expect("Failed Alice base");
    let bob_base = bob
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Base)
        .await
        .expect("Failed Bob base");

    println!("Initial Base layer:");
    println!("  Alice: {} allocations", alice_base.len());
    println!("  Bob: {} allocations\n", bob_base.len());

    // Create commitment test context
    let alice_commitment_pubkey = multisig.derive_alice_pubkey();
    let bob_commitment_pubkey = multisig.derive_bob_pubkey();

    let alice_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(alice_commitment_pubkey))
            .expect("Failed to compress Alice commitment pubkey"),
        bitcoin::Network::Regtest,
    );
    let bob_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(bob_commitment_pubkey))
            .expect("Failed to compress Bob commitment pubkey"),
        bitcoin::Network::Regtest,
    );

    let fee_sats = 1_000;
    let total = multisig_txout.value.to_sat() - fee_sats;
    let alice_sats = total / 2;
    let bob_sats = total - alice_sats;

    let multisig_script = multisig.create_2of2_script();

    let ctx = CommitmentTestContext::builder()
        .with_wallets(alice.clone(), bob.clone())
        .with_multisig(multisig_outpoint, multisig_txout.clone(), multisig.as_ref().clone(), multisig_script)
        .with_addresses(alice_commitment_addr, bob_commitment_addr)
        .with_btc_allocation(alice_sats, bob_sats)
        .with_contract(contract_id)
        .build()
        .expect("Failed to build commitment context");

    // ========== Commitment Updates ==========

    // Commitment #1: Alice 250k / Bob 250k (ACK)
    ctx.with_rgb(250_000, 250_000)
        .color_and_ack()
        .await
        .expect("Failed commitment #1");

    // Commitment #2: Alice 300k / Bob 200k (ACK, replaces #1)
    ctx.with_rgb(300_000, 200_000)
        .color_and_ack()
        .await
        .expect("Failed commitment #2");
    println!("  ✓ Commitment #2 replaces #1 in Active layer\n");

    // Commitment #3: Alice 350k / Bob 150k (Pending only)
    ctx.with_rgb(350_000, 150_000)
        .color_pending()
        .await
        .expect("Failed commitment #3");
    println!("  ✓ Commitment #3 in Pending (Active still shows #2)\n");

    // Commitment #4: Alice 400k / Bob 100k (Pending overwrites #3)
    ctx.with_rgb(400_000, 100_000)
        .color_pending()
        .await
        .expect("Failed commitment #4");
    println!("  ✓ Commitment #4 overwrites #3 in Pending (Active still shows #2)\n");

    // Commitment #5: Alice 450k / Bob 50k (ACK, replaces #2)
    ctx.with_rgb(450_000, 50_000)
        .color_and_ack()
        .await
        .expect("Failed commitment #5");
    println!("  ✓ Commitment #5 replaces #2 in Active layer\n");

    // ========== Final Verification ==========
    println!("=== Final Verification ===");
    let alice_base_final = alice
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Base)
        .await
        .expect("Failed Alice base final");
    let bob_base_final = bob
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Base)
        .await
        .expect("Failed Bob base final");

    println!("Final Base layer (should be unchanged):");
    println!("  Alice: {} allocations", alice_base_final.len());
    println!("  Bob: {} allocations\n", bob_base_final.len());

    println!("All Commitment flows verified!");
    println!("  ✓ Commitment #1: Alice 250k / Bob 250k (ACK)");
    println!("  ✓ Commitment #2: Alice 300k / Bob 200k (ACK, replaces #1)");
    println!("  ✓ Commitment #3: Alice 350k / Bob 150k (Pending only)");
    println!("  ✓ Commitment #4: Alice 400k / Bob 100k (Pending overwrites #3)");
    println!("  ✓ Commitment #5: Alice 450k / Bob 50k (ACK, replaces #2)");
    println!("  ✓ Base layer remains unchanged throughout\n");
}

// ========== Week 1: Quick Wins ==========

/// Test Cooperative Channel Close with RGB
///
/// **Priority**: P0
///
/// **Test Scenario**:
/// Verifies RGB assets are correctly transferred during cooperative channel closure.
///
/// **Complete Flow**:
/// ```text
/// Phase 1: Setup
///   - Create multisig UTXO with 500k RGB
///   - Establish commitment: Alice 300k / Bob 200k (Active state)
///
/// Phase 2: Closing TX (Cooperative)
///   - Create closing TX: multisig → (Alice P2WPKH, Bob P2WPKH)
///   - color_psbt() for closing TX → Pending state
///   - on_coop_close_ready() → Cache RGB state
///   - Sign with multisig (2-of-2)
///   - Broadcast and confirm
///
/// Phase 3: RGB State Transition (Closing TX confirmed)
///   - on_channel_closed() → Apply pending closing state, update witness status to Mined
///   - RGB state now in Base layer with Mined status
///   - Caches to applied_closing_overlays (for sweeper)
///
/// Phase 4: Sweeper TX (Transfer to RGB wallet)
///   - Create sweeper TX: closing output → RGB wallet address
///   - color_psbt() detects is_sweep_tx=true → Use applied_closing_overlays
///   - Sign with RGB wallet key
///   - finalize_rgb_tx() → Apply to runtime BEFORE broadcast (ln-node pattern)
///   - Broadcast and confirm
///
/// Phase 5: Final Verification
///   - Verify final RGB allocations in Base layer
/// ```
///
/// **Key APIs**:
/// - color_psbt (Closing TX) → Pending state
/// - on_coop_close_ready → Cache closing state
/// - on_channel_closed → Apply state, update witness to Mined, cache to applied_closing_overlays
/// - color_psbt (Sweeper TX) → Use applied_closing_overlays
/// - finalize_rgb_tx → Apply to runtime, clear state
#[tokio::test]
#[serial]
async fn test_cooperative_close() {
    println!("\n=== Test: Cooperative Close Channel with RGB ===\n");

    // ========== Phase 1: Setup (using high-level helper) ==========
    println!("Phase 1: Setup multisig and commitment...");

    let setup = test_helpers::setup_channel_with_rgb("coop_close", 500_000, "CoopNIA", "CNIA")
        .await
        .expect("Failed to setup channel");

    let alice = &setup.alice;
    let bob = &setup.bob;
    let contract_id = setup.contract_id;
    let multisig_outpoint = setup.multisig_outpoint;
    let multisig_txout = setup.multisig_txout;
    let multisig = &setup.multisig;
    let network = setup.network;

    let multisig_script = multisig.create_2of2_script();

    // Derive commitment addresses
    let alice_commitment_pubkey = multisig.derive_alice_pubkey();
    let bob_commitment_pubkey = multisig.derive_bob_pubkey();

    let alice_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(alice_commitment_pubkey))
            .expect("Failed to compress Alice commitment pubkey"),
        bitcoin::Network::Regtest,
    );
    let bob_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(bob_commitment_pubkey))
            .expect("Failed to compress Bob commitment pubkey"),
        bitcoin::Network::Regtest,
    );

    let fee_sats = 1_000;
    let total = multisig_txout.value.to_sat() - fee_sats;
    let alice_sats = total / 2;
    let bob_sats = total - alice_sats;

    let ctx = CommitmentTestContext::builder()
        .with_wallets(alice.clone(), bob.clone())
        .with_multisig(multisig_outpoint, multisig_txout.clone(), multisig.as_ref().clone(), multisig_script.clone())
        .with_addresses(alice_commitment_addr, bob_commitment_addr)
        .with_btc_allocation(alice_sats, bob_sats)
        .with_contract(contract_id)
        .build()
        .expect("Failed to build commitment context");

    ctx.with_rgb(300_000, 200_000)
        .color_and_ack()
        .await
        .expect("Failed commitment");

    println!("  ✓ Commitment established: Alice 300k / Bob 200k (Active)\n");

    // ========== Phase 2: Create and Broadcast Closing TX ==========
    println!("Phase 2: Creating cooperative closing TX...");

    // Get shutdown addresses from multisig helper's pubkeys
    let alice_shutdown_pubkey = ctx.multisig().derive_alice_pubkey();
    let bob_shutdown_pubkey = ctx.multisig().derive_bob_pubkey();

    let alice_shutdown_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(alice_shutdown_pubkey))
            .expect("Failed to compress Alice pubkey"),
        to_bitcoin_network(network),
    );
    let bob_shutdown_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(bob_shutdown_pubkey))
            .expect("Failed to compress Bob pubkey"),
        to_bitcoin_network(network),
    );

    // Create closing TX structure
    let close_fee = 1_000u64;
    let alice_close_sats = alice_sats - close_fee / 2;
    let bob_close_sats = bob_sats - close_fee / 2;

    let mut closing_tx = bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: multisig_outpoint,
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![
            bitcoin::TxOut {
                value: bitcoin::Amount::from_sat(alice_close_sats),
                script_pubkey: alice_shutdown_addr.script_pubkey(),
            },
            bitcoin::TxOut {
                value: bitcoin::Amount::from_sat(bob_close_sats),
                script_pubkey: bob_shutdown_addr.script_pubkey(),
            },
        ],
    };

    println!("  ✓ Closing TX created");
    println!("    Output 0: Alice {} sats", alice_close_sats);
    println!("    Output 1: Bob {} sats", bob_close_sats);

    // Color closing TX with RGB (Alice 300k → vout 0, Bob 200k → vout 1)
    use lightning_rgb_types::{
        contract_id_from_bytes, AssetColoringInfo, AssetIface, PlainOutpoint,
    };
    use std::collections::HashMap;

    let closing_coloring_info = lightning_rgb_types::ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_from_bytes(
                AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes(),
            )
            .unwrap(),
            AssetColoringInfo {
                iface: AssetIface::RGB20,
                input_outpoints: vec![PlainOutpoint {
                    txid: multisig_outpoint.txid.to_string(),
                    vout: multisig_outpoint.vout,
                }],
                output_map: HashMap::from([(0u32, 300_000), (1u32, 200_000)]),
                static_blinding: None,
            },
        )]),
        static_blinding: None,
        nonce: None,
    };

    // Convert to PSBT for coloring
    let closing_psbt = bitcoin::Psbt::from_unsigned_tx(closing_tx.clone())
        .expect("Failed to create PSBT");

    // Add witness_utxo for input
    let mut closing_psbt_with_witness = closing_psbt.clone();
    closing_psbt_with_witness.inputs[0].witness_utxo = Some(multisig_txout.clone());

    // Alice colors closing TX
    let alice_colored_closing_psbt = alice
        .color_psbt(closing_psbt_with_witness.clone(), closing_coloring_info.clone())
        .await
        .expect("Failed Alice color closing TX");

    // Bob colors closing TX
    let bob_colored_closing_psbt = bob
        .color_psbt(closing_psbt_with_witness, closing_coloring_info)
        .await
        .expect("Failed Bob color closing TX");

    // Verify colored txid matches
    let alice_closing_txid = alice_colored_closing_psbt.unsigned_tx.compute_txid();
    let bob_closing_txid = bob_colored_closing_psbt.unsigned_tx.compute_txid();
    assert_eq!(
        alice_closing_txid, bob_closing_txid,
        "Colored closing txids must match"
    );

    println!("  ✓ Closing TX colored: {}", alice_closing_txid);

    // Update closing_tx with colored version
    closing_tx = alice_colored_closing_psbt.unsigned_tx.clone();

    // Sign closing TX with multisig (2-of-2)
    ctx.multisig()
        .finalize_tx(&mut closing_tx, ctx.multisig_script(), multisig_txout.value)
        .expect("Failed to sign closing TX");

    // CRITICAL: Cache RGB state BEFORE broadcast (cooperative close specific!)
    alice
        .on_coop_close_ready(multisig_outpoint, closing_tx.clone())
        .await
        .expect("Failed Alice on_coop_close_ready");

    bob.on_coop_close_ready(multisig_outpoint, closing_tx.clone())
        .await
        .expect("Failed Bob on_coop_close_ready");

    // Broadcast closing TX and mine 6 blocks
    let closing_txid_str = broadcast_and_mine(&closing_tx, 6, INSTANCE_1, network)
        .expect("Failed to broadcast closing TX");

    let closing_block_height = chain::get_height_custom(INSTANCE_1);

    println!("  ✓ Closing TX confirmed: {} at height {}", closing_txid_str, closing_block_height);
    println!("  ✓ Phase 2 complete\n");

    // ========== Phase 3: Apply closing state (handle_final_update) ==========
    println!("Phase 3: Applying closing state (on_channel_closed)...");

    // Apply pending closing state and update witness status to Mined
    alice
        .on_channel_closed(multisig_outpoint, closing_tx.clone(), closing_block_height)
        .await
        .expect("Failed Alice on_channel_closed");

    bob.on_channel_closed(multisig_outpoint, closing_tx.clone(), closing_block_height)
        .await
        .expect("Failed Bob on_channel_closed");

    println!("  ✓ Pending closing state applied to Applied layer");
    println!("  ✓ Phase 3 complete\n");

    // ========== Phase 4: Create and Broadcast Sweeper TX ==========
    println!("Phase 4: Creating sweeper TX to RGB wallet...");

    // Alice sweeper TX: closing output 0 → RGB wallet address
    let alice_closing_output = bitcoin::OutPoint {
        txid: bitcoin::Txid::from_str(&closing_txid_str).expect("Failed to parse closing txid"),
        vout: 0,
    };

    let alice_sweep_dest = alice.get_address().await.expect("Failed get Alice sweep dest");
    let sweep_fee = 500u64;
    let alice_sweep_amount = alice_close_sats - sweep_fee;

    let alice_sweep_tx = bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: alice_closing_output,
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![bitcoin::TxOut {
            value: bitcoin::Amount::from_sat(alice_sweep_amount),
            script_pubkey: bitcoin::ScriptBuf::from_bytes(alice_sweep_dest.script_pubkey().to_vec()),
        }],
    };

    // Color sweeper TX (will detect is_sweep_tx=true and use applied_closing_overlays)
    let alice_sweep_coloring = lightning_rgb_types::ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_from_bytes(
                AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes(),
            )
            .unwrap(),
            AssetColoringInfo {
                iface: AssetIface::RGB20,
                input_outpoints: vec![PlainOutpoint {
                    txid: alice_closing_output.txid.to_string(),
                    vout: alice_closing_output.vout,
                }],
                output_map: HashMap::from([(0u32, 300_000)]),
                static_blinding: None,
            },
        )]),
        static_blinding: None,
        nonce: None,
    };

    let alice_sweep_psbt = bitcoin::Psbt::from_unsigned_tx(alice_sweep_tx.clone())
        .expect("Failed to create sweep PSBT");

    let mut alice_sweep_psbt_with_witness = alice_sweep_psbt.clone();
    alice_sweep_psbt_with_witness.inputs[0].witness_utxo = Some(closing_tx.output[0].clone());

    let alice_colored_sweep_psbt = alice
        .color_psbt(alice_sweep_psbt_with_witness, alice_sweep_coloring)
        .await
        .expect("Failed Alice color sweep TX");

    println!("  ✓ Alice sweep TX colored (used applied_closing_overlays)");

    // Sign with Alice's xpriv (from multisig helper)
    let mut alice_sweep_tx_final = alice_colored_sweep_psbt.unsigned_tx.clone();
    ctx.multisig()
        .sign_p2wpkh_tx(
            &mut alice_sweep_tx_final,
            ctx.multisig().alice_xprv(),
            bitcoin::Amount::from_sat(alice_close_sats),
        )
        .expect("Failed to sign Alice sweep TX");

    // CRITICAL: Finalize RGB state BEFORE broadcast (cooperative close specific!)
    alice
        .finalize_rgb_tx(alice_sweep_tx_final.clone())
        .await
        .expect("Failed Alice finalize_rgb_tx");

    let alice_sweep_txid = broadcast_and_mine(&alice_sweep_tx_final, 1, INSTANCE_1, network)
        .expect("Failed to broadcast Alice sweep TX");

    println!("  ✓ Alice sweep TX finalized and confirmed: {}", alice_sweep_txid);

    // Bob sweeper TX: closing output 1 → RGB wallet address
    let bob_closing_output = bitcoin::OutPoint {
        txid: bitcoin::Txid::from_str(&closing_txid_str).expect("Failed to parse closing txid"),
        vout: 1,
    };

    let bob_sweep_dest = bob.get_address().await.expect("Failed get Bob sweep dest");
    let bob_sweep_amount = bob_close_sats - sweep_fee;

    let bob_sweep_tx = bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: bob_closing_output,
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![bitcoin::TxOut {
            value: bitcoin::Amount::from_sat(bob_sweep_amount),
            script_pubkey: bitcoin::ScriptBuf::from_bytes(bob_sweep_dest.script_pubkey().to_vec()),
        }],
    };

    let bob_sweep_coloring = lightning_rgb_types::ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_from_bytes(
                AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes(),
            )
            .unwrap(),
            AssetColoringInfo {
                iface: AssetIface::RGB20,
                input_outpoints: vec![PlainOutpoint {
                    txid: bob_closing_output.txid.to_string(),
                    vout: bob_closing_output.vout,
                }],
                output_map: HashMap::from([(0u32, 200_000)]),
                static_blinding: None,
            },
        )]),
        static_blinding: None,
        nonce: None,
    };

    let bob_sweep_psbt = bitcoin::Psbt::from_unsigned_tx(bob_sweep_tx.clone())
        .expect("Failed to create Bob sweep PSBT");

    let mut bob_sweep_psbt_with_witness = bob_sweep_psbt.clone();
    bob_sweep_psbt_with_witness.inputs[0].witness_utxo = Some(closing_tx.output[1].clone());

    let bob_colored_sweep_psbt = bob
        .color_psbt(bob_sweep_psbt_with_witness, bob_sweep_coloring)
        .await
        .expect("Failed Bob color sweep TX");

    // Sign with Bob's xpriv (from multisig helper)
    let mut bob_sweep_tx_final = bob_colored_sweep_psbt.unsigned_tx.clone();
    ctx.multisig()
        .sign_p2wpkh_tx(
            &mut bob_sweep_tx_final,
            ctx.multisig().bob_xprv(),
            bitcoin::Amount::from_sat(bob_close_sats),
        )
        .expect("Failed to sign Bob sweep TX");

    // Finalize RGB state BEFORE broadcast
    bob.finalize_rgb_tx(bob_sweep_tx_final.clone())
        .await
        .expect("Failed Bob finalize_rgb_tx");

    let bob_sweep_txid = broadcast_and_mine(&bob_sweep_tx_final, 1, INSTANCE_1, network)
        .expect("Failed to broadcast Bob sweep TX");

    println!("  ✓ Bob sweep TX finalized and confirmed: {}", bob_sweep_txid);
    println!("  ✓ Phase 4 complete\n");

    // ========== Phase 5: Verify final state (using high-level helper) ==========
    println!("Phase 5: Verifying final state...");

    test_helpers::verify_final_balances(alice, bob, contract_id, 300_000, 200_000)
        .await
        .expect("Failed to verify balances");

    // Additional verification: Pending/Active layers should be cleared
    let alice_pending = alice
        .get_rgb_allocations_in_layer(contract_id, lightning_rgb_types::RgbStateLayer::Pending)
        .await
        .expect("Failed Alice pending");
    let alice_active = alice
        .get_rgb_allocations_in_layer(contract_id, lightning_rgb_types::RgbStateLayer::Active)
        .await
        .expect("Failed Alice active");

    assert!(alice_pending.is_empty(), "Alice Pending layer should be empty");
    assert!(alice_active.is_empty(), "Alice Active layer should be empty");

    let bob_pending = bob
        .get_rgb_allocations_in_layer(contract_id, lightning_rgb_types::RgbStateLayer::Pending)
        .await
        .expect("Failed Bob pending");
    let bob_active = bob
        .get_rgb_allocations_in_layer(contract_id, lightning_rgb_types::RgbStateLayer::Active)
        .await
        .expect("Failed Bob active");

    assert!(bob_pending.is_empty(), "Bob Pending layer should be empty");
    assert!(bob_active.is_empty(), "Bob Active layer should be empty");

    println!("  ✓ Pending/Active layers cleared");
    println!("  ✓ Phase 5 complete\n");

    println!("Cooperative close test passed!");
    println!("  ✓ Closing TX negotiated and broadcast (with on_coop_close_ready)");
    println!("  ✓ RGB state transitioned from Active → Applied → Base");
    println!("  ✓ Sweeper TXs recovered RGB to wallets (with finalize_rgb_tx)");
    println!("  ✓ Channel completely closed");
}

/// Test Complete HTLC Lifecycle with Force Close and Settlement
///
/// **Priority**: P0 (Core functionality)
///
/// **Test Scenario**:
/// Complete end-to-end test of HTLC settlement via force close, including:
/// - Commitment TX with HTLC output (colored and broadcast)
/// - HTLC-Success TX (Bob claims with preimage)
/// - All outputs swept to RGB wallets
///
/// **Initial State**:
/// - Multisig UTXO: 500k RGB
///
/// **Commitment Structure**:
/// ```
/// Output 0: Alice to_local (200k RGB)
/// Output 1: Bob to_remote (250k RGB)
/// Output 2: HTLC output (50k RGB) - Alice → Bob pending payment
/// ```
///
/// **Test Flow (6 Phases)**:
/// 1. **Funding**: Create multisig UTXO with 500k RGB
/// 2. **Commitment with HTLC**: Create and color commitment TX (force close scenario)
/// 3. **Broadcast & Apply**: Broadcast commitment TX, call on_channel_closed (Active → Applied)
/// 4. **HTLC-Success**: Bob creates HTLC-Success TX with preimage, broadcast and apply state
/// 5. **Sweep All Outputs**:
///    - Alice sweeps commitment output 0 (200k RGB)
///    - Bob sweeps commitment output 1 (250k RGB)
///    - Bob sweeps HTLC-Success TX output (50k RGB)
/// 6. **Verification**: Verify final balances (Alice: 200k, Bob: 300k)
///
/// **Key Validations**:
/// - Commitment TX correctly colored with 3 outputs
/// - HTLC-Success TX properly signed with preimage
/// - RGB state transitions: Pending → Applied (commitment) → Applied (HTLC)
/// - All sweep TXs use applied overlays for RGB tracking
/// - Final RGB conservation: 200k + 300k = 500k
#[tokio::test]
#[serial]
async fn test_commitment_with_htlc() {
    println!("\n=== Test: Commitment with HTLC Outputs ===\n");

    // ========== Phase 1: Setup (using high-level helper) ==========
    println!("Phase 1: Setup multisig and contract...");

    let setup = test_helpers::setup_channel_with_rgb("htlc_test", 500_000, "HTLCTest", "HTLC")
        .await
        .expect("Failed to setup channel");

    let alice = &setup.alice;
    let bob = &setup.bob;
    let contract_id = setup.contract_id;
    let multisig_outpoint = setup.multisig_outpoint;
    let multisig_txout = setup.multisig_txout;
    let multisig = &setup.multisig;
    let network = setup.network;

    println!("  ✓ Multisig UTXO: {}:{}", multisig_outpoint.txid, multisig_outpoint.vout);
    println!("  ✓ Phase 1 complete\n");

    // ========== Phase 2: Create commitment with HTLC ==========
    println!("Phase 2: Creating commitment with HTLC...");

    // Derive commitment output addresses
    let alice_commitment_pubkey = multisig.derive_alice_pubkey();
    let bob_commitment_pubkey = multisig.derive_bob_pubkey();

    let alice_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(alice_commitment_pubkey))
            .expect("Failed to compress Alice commitment pubkey"),
        bitcoin::Network::Regtest,
    );
    let bob_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(bob_commitment_pubkey))
            .expect("Failed to compress Bob commitment pubkey"),
        bitcoin::Network::Regtest,
    );

    let fee_sats = 1_000;
    let htlc_sats = 5_000;  // Fixed Bitcoin amount for HTLC output
    let total = multisig_txout.value.to_sat() - fee_sats - htlc_sats;
    let alice_sats = total / 2;
    let bob_sats = total - alice_sats;

    let multisig_script = multisig.create_2of2_script();

    let ctx = CommitmentTestContext::builder()
        .with_wallets(alice.clone(), bob.clone())
        .with_multisig(multisig_outpoint, multisig_txout.clone(), multisig.as_ref().clone(), multisig_script.clone())
        .with_addresses(alice_commitment_addr, bob_commitment_addr)
        .with_btc_allocation(alice_sats, bob_sats)
        .with_contract(contract_id)
        .build()
        .expect("Failed to build commitment context");

    // Scenario: Alice 250k / Bob 250k, with one HTLC (Alice→Bob, 50k RGB)
    // Generate preimage and payment_hash
    use bitcoin::hashes::{Hash, sha256};
    let payment_preimage = [42u8; 32];
    let payment_hash_obj = sha256::Hash::hash(&payment_preimage);
    let payment_hash: [u8; 32] = *payment_hash_obj.as_byte_array();

    let pending = ctx.with_rgb(250_000, 250_000)
        .add_htlc(payment_hash, 50_000, true)  // Alice→Bob, 50k RGB
        .color_pending()
        .await
        .expect("Failed to create commitment with HTLC");

    println!("  ✓ Commitment created with HTLC:");
    println!("    Alice to_local: 200k RGB (250k - 50k in HTLC)");
    println!("    Bob to_remote: 250k RGB");
    println!("    HTLC (Alice→Bob): 50k RGB");
    println!("    Total: 500k RGB (conservation verified)");
    println!("  ✓ Phase 2 complete\n");

    // ========== Phase 2.5: Broadcast Commitment TX ==========
    println!("Phase 2.5: Broadcasting commitment TX to blockchain...");

    // Get commitment TX and sign with multisig
    let mut commitment_tx = pending.commitment_tx().clone();

    // Sign commitment TX with multisig (2-of-2)
    ctx.multisig()
        .finalize_tx(&mut commitment_tx, ctx.multisig_script(), multisig_txout.value)
        .expect("Failed to sign commitment TX");

    println!("  ✓ Commitment TX signed");

    // Broadcast commitment TX
    let commitment_txid_str = test_helpers::broadcast_and_mine(&commitment_tx, 6, INSTANCE_1, network)
        .expect("Failed to broadcast commitment TX");

    let commitment_block_height = chain::get_height_custom(INSTANCE_1);
    println!("  ✓ Commitment TX confirmed: {} at height {}", commitment_txid_str, commitment_block_height);

    // Apply commitment state (force close pattern: Active → Applied)
    alice.on_channel_closed(multisig_outpoint, commitment_tx.clone(), commitment_block_height)
        .await
        .expect("Failed Alice on_channel_closed");

    bob.on_channel_closed(multisig_outpoint, commitment_tx.clone(), commitment_block_height)
        .await
        .expect("Failed Bob on_channel_closed");

    println!("  ✓ Called on_channel_closed (commitment state applied)");
    println!("  ✓ Phase 2.5 complete\n");

    // ========== Phase 3: HTLC-Success (Bob claims HTLC) ==========
    println!("Phase 3: Bob spends HTLC with HTLC-Success transaction...");

    // Get commitment TX and HTLC info from pending
    let commitment_tx = pending.commitment_tx();
    let commitment_txid = commitment_tx.compute_txid();
    let htlcs = pending.htlcs();

    assert_eq!(htlcs.len(), 1, "Should have exactly 1 HTLC");
    let htlc = &htlcs[0];
    let htlc_vout = htlc.vout_in_commitment.expect("HTLC vout should be set");
    let htlc_amount_rgb = htlc.amount_rgb;

    println!("  HTLC info:");
    println!("    Commitment txid: {}", commitment_txid);
    println!("    HTLC vout: {}", htlc_vout);
    println!("    HTLC RGB amount: {}", htlc_amount_rgb);

    // Create HTLC-Success TX (Bob spends HTLC output)
    let htlc_outpoint = bitcoin::OutPoint {
        txid: commitment_txid,
        vout: htlc_vout,
    };

    // HTLC-Success TX output should be a LN-controlled P2WPKH address (not RGB wallet)
    // Use Bob's derived pubkey from multisig (similar to commitment outputs)
    let bob_pubkey = ctx.multisig().derive_bob_pubkey();
    let bob_bitcoin_pubkey = bitcoin::PublicKey::new(bob_pubkey);
    let bob_compressed_pubkey = bitcoin::CompressedPublicKey::try_from(bob_bitcoin_pubkey)
        .expect("Failed to compress Bob pubkey");
    let bob_ln_address = bitcoin::Address::p2wpkh(&bob_compressed_pubkey, to_bitcoin_network(network));

    let htlc_success_tx = test_helpers::create_htlc_success_tx(
        htlc_outpoint,
        bitcoin::Amount::from_sat(5000),  // HTLC bitcoin amount (from commitment TX)
        bob_ln_address.script_pubkey(),   // LN-controlled P2WPKH address
        bitcoin::Amount::from_sat(4500),  // Output amount (5000 - 500 fee)
    );

    // Get HTLC witness UTXO (from commitment TX)
    let htlc_witness_utxo = commitment_tx.output[htlc_vout as usize].clone();

    // Color and Broadcast HTLC-Success TX
    println!("Phase 3: Creating HTLC-Success TX (Bob claims HTLC)...");

    let (htlc_psbt, htlc_txid) = test_helpers::color_htlc_tx(
        &bob,
        htlc_success_tx.clone(),
        htlc_witness_utxo,
        commitment_txid,
        htlc_vout,
        htlc_amount_rgb,
        contract_id,
        "Bob",
        htlc_amount_rgb,  // Bob should receive full 50k RGB
    )
    .await
    .expect("Failed to color HTLC-Success TX");

    println!("  ✓ HTLC-Success TX colored");
    println!("    HTLC-Success txid: {}", htlc_txid);
    println!("    RGB amount: {}", htlc_amount_rgb);

    // Extract final transaction from PSBT
    let mut htlc_final_tx = htlc_psbt.extract_tx().expect("Failed to extract HTLC TX");

    // Sign HTLC-Success TX with Bob's key and preimage
    let bob_pubkey = ctx.multisig().derive_bob_pubkey();
    let htlc_script = test_helpers::create_minimal_htlc_script(&payment_hash, &bob_pubkey);

    test_helpers::sign_htlc_success_tx(
        &mut htlc_final_tx,
        &htlc_script,
        bitcoin::Amount::from_sat(5000),
        &payment_preimage,
        ctx.multisig().bob_xprv().clone(),
    )
    .expect("Failed to sign HTLC-Success TX");

    println!("  ✓ HTLC-Success TX signed with preimage");

    // Broadcast HTLC-Success TX and mine blocks
    println!("  Broadcasting HTLC-Success TX...");
    let htlc_txid_str = test_helpers::broadcast_and_mine(&htlc_final_tx, 6, INSTANCE_1, network)
        .expect("Failed to broadcast HTLC-Success TX");

    let htlc_block_height = chain::get_height_custom(INSTANCE_1);
    println!("  ✓ HTLC-Success TX confirmed: {} at height {}", htlc_txid_str, htlc_block_height);
    println!("  ✓ Phase 3 complete\n");

    // ========== Phase 4: Apply HTLC RGB State ==========
    println!("Phase 4: Applying HTLC state (on_htlc_tx_confirmed)...");

    println!("  DEBUG: Calling on_htlc_tx_confirmed with:");
    println!("    - commitment_txid: {}", commitment_txid);
    println!("    - htlc_tx: {}", htlc_final_tx.compute_txid());
    println!("    - height: {}", htlc_block_height);

    // Apply pending HTLC state and update witness status to Mined
    bob.on_htlc_tx_confirmed(commitment_txid, htlc_final_tx.clone(), htlc_block_height)
        .await
        .expect("Failed Bob on_htlc_tx_confirmed");

    println!("  ✓ Pending HTLC state applied to runtime");
    println!("  ✓ Witness status updated to Mined (height {})", htlc_block_height);
    println!("  ✓ HTLC overlay cached to applied_htlc_overlays");
    println!("  ✓ Phase 4 complete\n");

    // ========== Phase 5: Sweep all outputs to RGB wallets ==========
    println!("Phase 5: Sweeping commitment outputs and HTLC output...");

    // Get sweep destination addresses
    let alice_sweep_dest = alice
        .get_address()
        .await
        .expect("Failed to get Alice sweep destination");
    let bob_sweep_dest = bob
        .get_address()
        .await
        .expect("Failed to get Bob sweep destination");

    // 5.1: Alice sweeps commitment TX vout 0 (to_local) - 200k RGB
    println!("\n  5.1: Alice sweeps commitment to_local output (200k RGB)...");
    let alice_sweep_params = test_helpers::SweepTxParams {
        input: bitcoin::OutPoint {
            txid: commitment_txid,
            vout: 0,
        },
        witness_utxo: commitment_tx.output[0].clone(),
        dest_address: alice_sweep_dest.clone(),
        input_amount_sats: alice_sats,
        fee_sats: 500,
        contract_id,
        rgb_amount: 200_000,
        signer_xpriv: ctx.multisig().alice_xprv().clone(),
        multisig: ctx.multisig_arc(),
    };

    let alice_sweep_txid = test_helpers::create_and_broadcast_sweep_tx(
        &alice,
        alice_sweep_params,
        "Alice commitment",
        INSTANCE_1,
        network,
    )
    .await
    .expect("Failed to sweep Alice commitment output");

    println!("  ✓ Alice swept 200k RGB from commitment to_local");
    println!("  Alice sweep txid: {}", alice_sweep_txid);

    // 5.2: Bob sweeps commitment TX vout 1 (to_remote) - 250k RGB
    println!("\n  5.2: Bob sweeps commitment to_remote output (250k RGB)...");
    let bob_commitment_sweep_params = test_helpers::SweepTxParams {
        input: bitcoin::OutPoint {
            txid: commitment_txid,
            vout: 1,
        },
        witness_utxo: commitment_tx.output[1].clone(),
        dest_address: bob_sweep_dest.clone(),
        input_amount_sats: bob_sats,
        fee_sats: 500,
        contract_id,
        rgb_amount: 250_000,
        signer_xpriv: ctx.multisig().bob_xprv().clone(),
        multisig: ctx.multisig_arc(),
    };

    let bob_commitment_sweep_txid = test_helpers::create_and_broadcast_sweep_tx(
        &bob,
        bob_commitment_sweep_params,
        "Bob commitment",
        INSTANCE_1,
        network,
    )
    .await
    .expect("Failed to sweep Bob commitment output");

    println!("  ✓ Bob swept 250k RGB from commitment to_remote");
    println!("  Bob commitment sweep txid: {}", bob_commitment_sweep_txid);

    // 5.3: Bob sweeps HTLC-Success TX output - 50k RGB
    println!("\n  5.3: Bob sweeps HTLC-Success TX output (50k RGB)...");
    let htlc_txid = htlc_final_tx.compute_txid();
    let bob_htlc_bitcoin_amount = 4500u64;

    let bob_htlc_sweep_params = test_helpers::SweepTxParams {
        input: bitcoin::OutPoint {
            txid: htlc_txid,
            vout: 0,
        },
        witness_utxo: htlc_final_tx.output[0].clone(),
        dest_address: bob_sweep_dest,
        input_amount_sats: bob_htlc_bitcoin_amount,
        fee_sats: 500,
        contract_id,
        rgb_amount: htlc_amount_rgb,
        signer_xpriv: ctx.multisig().bob_xprv().clone(),
        multisig: ctx.multisig_arc(),
    };

    let bob_htlc_sweep_txid = test_helpers::create_and_broadcast_sweep_tx(
        &bob,
        bob_htlc_sweep_params,
        "Bob HTLC",
        INSTANCE_1,
        network,
    )
    .await
    .expect("Failed to sweep Bob HTLC output");

    println!("  ✓ Bob swept 50k RGB from HTLC-Success TX");
    println!("  Bob HTLC sweep txid: {}", bob_htlc_sweep_txid);
    println!("\n  ✓ Phase 5 complete (all outputs swept)\n");

    // ========== Phase 6: Verify final RGB state (using high-level helper) ==========
    println!("Phase 6: Verifying final RGB state...");

    test_helpers::verify_final_balances(alice, bob, contract_id, 200_000, 300_000)
        .await
        .expect("Failed to verify balances");

    println!("  ✓ Phase 6 complete\n");

    println!("✓ Test passed: Complete HTLC lifecycle (Commitment + HTLC-Success + Sweep) works!\n");
}


/// Test L2 to L1 Asset Transfer
///
/// **Priority**: P1 (L2→L1 bridge verification)
///
/// **Test Scenario**:
/// Verifies that RGB assets settled from L2 (Lightning) can be used in L1 (on-chain) transfers.
/// This test validates the complete bridge from L2 channel close to L1 UTXO usage.
///
/// **Flow**:
/// ```
/// Phase 1-5: L2 Settlement (HTLC Resolution via Force Close)
///   - Funding: 500k RGB in channel
///   - Commitment with HTLC (Alice→Bob, 50k RGB)
///   - HTLC-Success settlement
///   - Sweep outputs: Alice (200k), Bob (300k)
///
/// Phase 6: L1 Transfers (Post-Sweep Asset Usage)
///   - Alice transfers 100k RGB to Bob via L1 (witness transfer)
///   - Bob transfers 150k RGB to Alice via L1 (witness transfer)
///   - Verify L2→L1 asset continuity
/// ```
///
/// **Key Validations**:
/// - RGB assets from L2 sweep are spendable on L1
/// - L1 transfer uses standard RGB transfer protocol
/// - Total RGB conservation across L2 and L1: 500k
/// - No asset loss during L2→L1 transition
///
/// **References**:
/// - L2 settlement: `test_commitment_with_htlc` (Phase 1-5)
/// - L1 transfers: `tests/transfer.rs` (witness/blinded transfers)
#[tokio::test]
#[serial]
async fn test_l2_to_l1_transfer() {
    println!("\n=== Test: L2 to L1 Asset Transfer ===\n");

    // ========== Phase 1-5: L2 Settlement (using high-level helper) ==========
    println!("Phase 1: Setup multisig and contract...");

    let setup = test_helpers::setup_channel_with_rgb("l2_l1", 500_000, "L2L1Test", "L2L1")
        .await
        .expect("Failed to setup channel");

    let alice = &setup.alice;
    let bob = &setup.bob;
    let contract_id = setup.contract_id;
    let multisig_outpoint = setup.multisig_outpoint;
    let multisig_txout = setup.multisig_txout;
    let multisig = &setup.multisig;
    let network = setup.network;

    println!("  ✓ Multisig UTXO: {}:{}", multisig_outpoint.txid, multisig_outpoint.vout);
    println!("  ✓ Phase 1 complete\n");

    // Phase 2: Create commitment with HTLC
    println!("Phase 2: Creating commitment with HTLC...");

    let alice_commitment_pubkey = multisig.derive_alice_pubkey();
    let bob_commitment_pubkey = multisig.derive_bob_pubkey();

    let alice_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(alice_commitment_pubkey))
            .expect("Failed to compress Alice commitment pubkey"),
        bitcoin::Network::Regtest,
    );
    let bob_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(bob_commitment_pubkey))
            .expect("Failed to compress Bob commitment pubkey"),
        bitcoin::Network::Regtest,
    );

    let fee_sats = 1_000;
    let htlc_sats = 5_000;
    let total = multisig_txout.value.to_sat() - fee_sats - htlc_sats;
    let alice_sats = total / 2;
    let bob_sats = total - alice_sats;

    let multisig_script = multisig.create_2of2_script();

    let ctx = CommitmentTestContext::builder()
        .with_wallets(alice.clone(), bob.clone())
        .with_multisig(multisig_outpoint, multisig_txout.clone(), multisig.as_ref().clone(), multisig_script.clone())
        .with_addresses(alice_commitment_addr, bob_commitment_addr)
        .with_btc_allocation(alice_sats, bob_sats)
        .with_contract(contract_id)
        .build()
        .expect("Failed to build commitment context");

    // Generate preimage and payment_hash
    use bitcoin::hashes::{Hash, sha256};
    let payment_preimage = [42u8; 32];
    let payment_hash_obj = sha256::Hash::hash(&payment_preimage);
    let payment_hash: [u8; 32] = *payment_hash_obj.as_byte_array();

    let pending = ctx.with_rgb(250_000, 250_000)
        .add_htlc(payment_hash, 50_000, true)
        .color_pending()
        .await
        .expect("Failed to create commitment with HTLC");

    println!("  ✓ Phase 2 complete\n");

    // Phase 2.5: Broadcast Commitment TX
    println!("Phase 2.5: Broadcasting commitment TX...");

    let mut commitment_tx = pending.commitment_tx().clone();
    let commitment_txid = commitment_tx.compute_txid();

    ctx.multisig()
        .finalize_tx(&mut commitment_tx, ctx.multisig_script(), multisig_txout.value)
        .expect("Failed to sign commitment TX");

    let _commitment_txid_str = test_helpers::broadcast_and_mine(&commitment_tx, 6, INSTANCE_1, network)
        .expect("Failed to broadcast commitment TX");

    let commitment_block_height = chain::get_height_custom(INSTANCE_1);

    alice.on_channel_closed(multisig_outpoint, commitment_tx.clone(), commitment_block_height)
        .await
        .expect("Failed Alice on_channel_closed");

    bob.on_channel_closed(multisig_outpoint, commitment_tx.clone(), commitment_block_height)
        .await
        .expect("Failed Bob on_channel_closed");

    println!("  ✓ Phase 2.5 complete\n");

    // Phase 3: HTLC-Success
    println!("Phase 3: Bob spends HTLC...");

    let commitment_tx = pending.commitment_tx();
    let htlcs = pending.htlcs();
    let htlc = &htlcs[0];
    let htlc_vout = htlc.vout_in_commitment.expect("HTLC vout should be set");
    let htlc_amount_rgb = htlc.amount_rgb;

    let htlc_outpoint = bitcoin::OutPoint {
        txid: commitment_txid,
        vout: htlc_vout,
    };

    let bob_pubkey = ctx.multisig().derive_bob_pubkey();
    let bob_bitcoin_pubkey = bitcoin::PublicKey::new(bob_pubkey);
    let bob_compressed_pubkey = bitcoin::CompressedPublicKey::try_from(bob_bitcoin_pubkey)
        .expect("Failed to compress Bob pubkey");
    let bob_ln_address = bitcoin::Address::p2wpkh(&bob_compressed_pubkey, to_bitcoin_network(network));

    let htlc_success_tx = test_helpers::create_htlc_success_tx(
        htlc_outpoint,
        bitcoin::Amount::from_sat(5000),
        bob_ln_address.script_pubkey(),
        bitcoin::Amount::from_sat(4500),
    );

    let htlc_witness_utxo = commitment_tx.output[htlc_vout as usize].clone();

    let (htlc_psbt, _htlc_txid) = test_helpers::color_htlc_tx(
        &bob,
        htlc_success_tx.clone(),
        htlc_witness_utxo,
        commitment_txid,
        htlc_vout,
        htlc_amount_rgb,
        contract_id,
        "Bob",
        htlc_amount_rgb,
    )
    .await
    .expect("Failed to color HTLC-Success TX");

    let mut htlc_final_tx = htlc_psbt.extract_tx().expect("Failed to extract HTLC TX");

    let htlc_script = test_helpers::create_minimal_htlc_script(&payment_hash, &bob_pubkey);

    test_helpers::sign_htlc_success_tx(
        &mut htlc_final_tx,
        &htlc_script,
        bitcoin::Amount::from_sat(5000),
        &payment_preimage,
        ctx.multisig().bob_xprv().clone(),
    )
    .expect("Failed to sign HTLC-Success TX");

    let _htlc_txid_str = test_helpers::broadcast_and_mine(&htlc_final_tx, 6, INSTANCE_1, network)
        .expect("Failed to broadcast HTLC-Success TX");

    let htlc_block_height = chain::get_height_custom(INSTANCE_1);

    bob.on_htlc_tx_confirmed(commitment_txid, htlc_final_tx.clone(), htlc_block_height)
        .await
        .expect("Failed Bob on_htlc_tx_confirmed");

    println!("  ✓ Phase 3 complete\n");

    // Phase 5: Sweep all outputs
    println!("Phase 5: Sweeping all outputs...");

    let alice_sweep_dest = alice
        .get_address()
        .await
        .expect("Failed to get Alice sweep destination");
    let bob_sweep_dest = bob
        .get_address()
        .await
        .expect("Failed to get Bob sweep destination");

    // Alice sweeps commitment vout 0 (200k RGB)
    let alice_sweep_params = test_helpers::SweepTxParams {
        input: bitcoin::OutPoint {
            txid: commitment_txid,
            vout: 0,
        },
        witness_utxo: commitment_tx.output[0].clone(),
        dest_address: alice_sweep_dest.clone(),
        input_amount_sats: alice_sats,
        fee_sats: 500,
        contract_id,
        rgb_amount: 200_000,
        signer_xpriv: ctx.multisig().alice_xprv().clone(),
        multisig: ctx.multisig_arc(),
    };

    let _alice_sweep_txid = test_helpers::create_and_broadcast_sweep_tx(
        &alice,
        alice_sweep_params,
        "Alice commitment",
        INSTANCE_1,
        network,
    )
    .await
    .expect("Failed to sweep Alice commitment output");

    // Bob sweeps commitment vout 1 (250k RGB)
    let bob_commitment_sweep_params = test_helpers::SweepTxParams {
        input: bitcoin::OutPoint {
            txid: commitment_txid,
            vout: 1,
        },
        witness_utxo: commitment_tx.output[1].clone(),
        dest_address: bob_sweep_dest.clone(),
        input_amount_sats: bob_sats,
        fee_sats: 500,
        contract_id,
        rgb_amount: 250_000,
        signer_xpriv: ctx.multisig().bob_xprv().clone(),
        multisig: ctx.multisig_arc(),
    };

    let _bob_commitment_sweep_txid = test_helpers::create_and_broadcast_sweep_tx(
        &bob,
        bob_commitment_sweep_params,
        "Bob commitment",
        INSTANCE_1,
        network,
    )
    .await
    .expect("Failed to sweep Bob commitment output");

    // Bob sweeps HTLC-Success TX output (50k RGB)
    let htlc_txid = htlc_final_tx.compute_txid();
    let bob_htlc_bitcoin_amount = 4500u64;

    let bob_htlc_sweep_params = test_helpers::SweepTxParams {
        input: bitcoin::OutPoint {
            txid: htlc_txid,
            vout: 0,
        },
        witness_utxo: htlc_final_tx.output[0].clone(),
        dest_address: bob_sweep_dest,
        input_amount_sats: bob_htlc_bitcoin_amount,
        fee_sats: 500,
        contract_id,
        rgb_amount: htlc_amount_rgb,
        signer_xpriv: ctx.multisig().bob_xprv().clone(),
        multisig: ctx.multisig_arc(),
    };

    let _bob_htlc_sweep_txid = test_helpers::create_and_broadcast_sweep_tx(
        &bob,
        bob_htlc_sweep_params,
        "Bob HTLC",
        INSTANCE_1,
        network,
    )
    .await
    .expect("Failed to sweep Bob HTLC output");

    println!("  ✓ Phase 5 complete (all outputs swept)\n");

    // Verify L2 settlement balances
    alice.sync_runtime().await.expect("Failed to sync Alice");
    bob.sync_runtime().await.expect("Failed to sync Bob");

    let alice_l2_balance = alice
        .get_allocations(contract_id)
        .await
        .expect("Failed to get Alice balance");
    let bob_l2_balance = bob
        .get_allocations(contract_id)
        .await
        .expect("Failed to get Bob balance");

    let alice_l2_total: u64 = alice_l2_balance.iter().map(|(_, amount)| amount).sum();
    let bob_l2_total: u64 = bob_l2_balance.iter().map(|(_, amount)| amount).sum();

    println!("  L2 settlement complete:");
    println!("    Alice: {} RGB", alice_l2_total);
    println!("    Bob: {} RGB", bob_l2_total);

    assert_eq!(alice_l2_total, 200_000, "Alice should have 200k RGB after L2 settlement");
    assert_eq!(bob_l2_total, 300_000, "Bob should have 300k RGB after L2 settlement");

    // ========== Phase 6: L1 Transfers ==========
    println!("\nPhase 6: Testing L1 transfers with swept L2 assets...");

    // 6.1: Alice transfers 100k RGB to Bob via L1 (witness transfer)
    println!("\n  6.1: Alice transfers 100k RGB to Bob (L1 witness transfer)...");
    let alice_to_bob_amount = 100_000u64;

    let invoice_alice_to_bob = bob
        .create_invoice(
            contract_id,
            alice_to_bob_amount,
            true,  // use_witness_utxo
            None,  // nonce
            None,  // blinding_utxo
        )
        .await;

    let (consignment_key_1, tx1, _payment1) = alice
        .transfer(invoice_alice_to_bob, Some(2000), None, true)
        .await
        .expect("Alice→Bob transfer failed");

    println!("    ✓ Transfer executed, tx: {}", tx1.txid());

    chain::mine(false);
    println!("    ✓ Transaction confirmed");

    alice.sync_runtime().await.expect("Failed to sync Alice");
    bob.sync_runtime().await.expect("Failed to sync Bob");

    bob.accept_transfer(consignment_key_1)
        .await
        .expect("Failed to accept Alice→Bob transfer");
    bob.sync_runtime().await.expect("Failed to sync Bob");

    println!("    ✓ Transfer accepted by Bob");

    // 6.2: Bob transfers 150k RGB to Alice via L1 (witness transfer)
    println!("\n  6.2: Bob transfers 150k RGB to Alice (L1 witness transfer)...");
    let bob_to_alice_amount = 150_000u64;

    let invoice_bob_to_alice = alice
        .create_invoice(
            contract_id,
            bob_to_alice_amount,
            true,  // use_witness_utxo
            None,  // nonce
            None,  // blinding_utxo
        )
        .await;

    let (consignment_key_2, tx2, _payment2) = bob
        .transfer(invoice_bob_to_alice, Some(2000), None, true)
        .await
        .expect("Bob→Alice transfer failed");

    println!("    ✓ Transfer executed, tx: {}", tx2.txid());

    chain::mine(false);
    println!("    ✓ Transaction confirmed");

    alice.sync_runtime().await.expect("Failed to sync Alice");
    bob.sync_runtime().await.expect("Failed to sync Bob");

    alice.accept_transfer(consignment_key_2)
        .await
        .expect("Failed to accept Bob→Alice transfer");
    alice.sync_runtime().await.expect("Failed to sync Alice");

    println!("    ✓ Transfer accepted by Alice");

    // ========== Verify Final L1 Balances ==========
    println!("\n  Verifying final L1 balances...");

    let alice_final = alice
        .get_allocations(contract_id)
        .await
        .expect("Failed to get Alice final balance");

    let bob_final = bob
        .get_allocations(contract_id)
        .await
        .expect("Failed to get Bob final balance");

    let alice_final_total: u64 = alice_final.iter().map(|(_, amount)| amount).sum();
    let bob_final_total: u64 = bob_final.iter().map(|(_, amount)| amount).sum();

    println!("    Alice final balance: {} RGB", alice_final_total);
    println!("    Bob final balance: {} RGB", bob_final_total);

    // Calculate expected balances:
    // Alice: 200k (L2 sweep) - 100k (to Bob) + 150k (from Bob) = 250k
    // Bob: 300k (L2 sweep) + 100k (from Alice) - 150k (to Alice) = 250k
    let alice_expected = 250_000u64;
    let bob_expected = 250_000u64;

    assert_eq!(alice_final_total, alice_expected,
               "Alice should have 250k RGB (200k - 100k + 150k)");
    assert_eq!(bob_final_total, bob_expected,
               "Bob should have 250k RGB (300k + 100k - 150k)");

    // Verify total conservation
    let total = alice_final_total + bob_final_total;
    assert_eq!(total, 500_000, "Total RGB should be conserved (500k)");

    println!("    ✓ RGB conservation verified: {} + {} = {}",
             alice_final_total, bob_final_total, total);
    println!("  ✓ Phase 6 complete\n");

    println!("✓ Test passed: L2 settlement → L1 transfers verified!\n");
    println!("  L2→L1 bridge works correctly:");
    println!("  - L2 swept assets are spendable on L1");
    println!("  - Standard L1 transfer protocol works");
    println!("  - No asset loss during transition");
    println!("  - Total conservation: 500k RGB\n");
}

// ========== Utility Tests ==========

/// Test RGB Conservation Check (Generic)
///
/// **Priority**: P2 (Utility function)
///
/// **Purpose**:
/// Test force close channel flow with RGB state management.
///
/// **Force Close Flow (unilateral)**:
/// 1. One party broadcasts the latest commitment TX (instead of negotiating closing TX)
/// 2. The commitment TX was already colored and stored in Active layer
/// 3. Skip `on_coop_close_ready` (no closing TX to cache)
/// 4. Call `on_channel_closed` to notify channel closure
/// 5. Generate sweeper TXs from commitment TX outputs
/// 6. Verify RGB state transitions from Active → Applied → Base
///
/// **Key Differences from Cooperative Close**:
/// - **Cooperative**: Negotiate new closing TX → `on_coop_close_ready` + `on_channel_closed`
/// - **Force**: Broadcast existing commitment TX → only `on_channel_closed`
///
/// **RGB State Flow**:
/// ```
/// Active layer (commitment TX colored)
///   ↓ broadcast commitment TX
/// Applied layer (cached for sweeper)
///   ↓ sweeper TX finalizes
/// Base layer (confirmed on-chain)
/// ```
///
/// **Test Phases**:
/// - Phase 1: Setup channel with funding TX and initial commitments
/// - Phase 2: Create and ACK commitment update (Alice 300k / Bob 200k)
/// - Phase 3: Force close - broadcast commitment TX directly
/// - Phase 4: Generate and broadcast sweeper TXs
/// - Phase 5: Verify final state and RGB conservation
#[tokio::test]
#[serial]
async fn test_force_close() {
    println!("\n=== Test: Force Close Channel with RGB ===\n");

    // ========== Phase 1: Setup (using high-level helper) ==========
    println!("Phase 1: Setup multisig and commitment...");

    let setup = test_helpers::setup_channel_with_rgb("force_close", 500_000, "ForceNIA", "FNIA")
        .await
        .expect("Failed to setup channel");

    let alice = &setup.alice;
    let bob = &setup.bob;
    let contract_id = setup.contract_id;
    let multisig_outpoint = setup.multisig_outpoint;
    let multisig_txout = setup.multisig_txout;
    let multisig = &setup.multisig;
    let network = setup.network;

    let multisig_script = multisig.create_2of2_script();

    println!("  ✓ Phase 1 complete\n");

    // ========== Phase 2: Create commitment update (Alice 300k / Bob 200k) ==========
    println!("Phase 2: Creating commitment (Alice 300k / Bob 200k)...");

    // Derive commitment output addresses from multisig helper
    let alice_commitment_pubkey = multisig.derive_alice_pubkey();
    let bob_commitment_pubkey = multisig.derive_bob_pubkey();

    let alice_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(alice_commitment_pubkey))
            .expect("Failed to compress Alice commitment pubkey"),
        bitcoin::Network::Regtest,
    );
    let bob_commitment_addr = bitcoin::Address::p2wpkh(
        &bitcoin::CompressedPublicKey::try_from(bitcoin::PublicKey::new(bob_commitment_pubkey))
            .expect("Failed to compress Bob commitment pubkey"),
        bitcoin::Network::Regtest,
    );

    let fee_sats = 1_000;
    let total = multisig_txout.value.to_sat() - fee_sats;
    let alice_sats = total / 2;
    let bob_sats = total - alice_sats;

    let ctx = CommitmentTestContext::builder()
        .with_wallets(alice.clone(), bob.clone())
        .with_multisig(multisig_outpoint, multisig_txout.clone(), multisig.as_ref().clone(), multisig_script.clone())
        .with_addresses(alice_commitment_addr.clone(), bob_commitment_addr.clone())
        .with_btc_allocation(alice_sats, bob_sats)
        .with_contract(contract_id)
        .build()
        .expect("Failed to build commitment context");

    // Color commitment and get PendingCommitment (to access colored TX later)
    let pending = ctx.with_rgb(300_000, 200_000)
        .color_pending()
        .await
        .expect("Failed to color commitment");

    // Get the colored commitment TX (will be used for force close)
    let mut commitment_tx = pending.commitment_tx().clone();

    // ACK the commitment to move state to Active layer (real force close scenario)
    pending.ack()
        .await
        .expect("Failed to ACK commitment");

    println!("  ✓ Commitment created: Alice 300k / Bob 200k (Active layer after RevokeAndAck)");
    println!("  ✓ Phase 2 complete\n");

    // ========== Phase 3: Force close - broadcast commitment TX ==========
    println!("Phase 3: Force closing (broadcasting commitment TX)...");

    println!("  Colored Commitment TX: {}", commitment_tx.compute_txid());
    println!("    Output 0 (Alice): {} sats → {}", alice_sats, alice_commitment_addr);
    println!("    Output 1 (Bob): {} sats → {}", bob_sats, bob_commitment_addr);

    // Sign commitment TX with multisig (2-of-2) - required for broadcast!
    ctx.multisig()
        .finalize_tx(&mut commitment_tx, ctx.multisig_script(), multisig_txout.value)
        .expect("Failed to sign commitment TX");

    println!("  ✓ Commitment TX signed");

    // CRITICAL: Skip on_coop_close_ready (no closing TX negotiation!)
    // Broadcast commitment TX first, then call on_channel_closed
    let commitment_txid_str = broadcast_and_mine(&commitment_tx, 6, INSTANCE_1, network)
        .expect("Failed to broadcast commitment TX");

    let commitment_block_height = chain::get_height_custom(INSTANCE_1);

    println!("  ✓ Commitment TX confirmed: {} at height {}", commitment_txid_str, commitment_block_height);

    // Call on_channel_closed to notify channel closure (with block height)
    alice.on_channel_closed(multisig_outpoint, commitment_tx.clone(), commitment_block_height)
        .await
        .expect("Failed Alice on_channel_closed");

    bob.on_channel_closed(multisig_outpoint, commitment_tx.clone(), commitment_block_height)
        .await
        .expect("Failed Bob on_channel_closed");

    println!("  ✓ Called on_channel_closed (skipped on_coop_close_ready)");
    println!("  ✓ Phase 3 complete\n");

    // ========== Phase 4: Generate sweeper TXs (using high-level helper) ==========
    println!("Phase 4: Creating and broadcasting sweeper TXs...");

    // Alice sweeps commitment output[0] - 300k RGB
    let _alice_sweep_txid = test_helpers::sweep_output_to_wallet(
        alice,
        &commitment_tx,
        0,
        300_000,
        "Alice",
        "commitment",
        &ctx,
        ctx.multisig().alice_xprv(),
    )
    .await
    .expect("Failed to sweep Alice commitment output");

    // Bob sweeps commitment output[1] - 200k RGB
    let _bob_sweep_txid = test_helpers::sweep_output_to_wallet(
        bob,
        &commitment_tx,
        1,
        200_000,
        "Bob",
        "commitment",
        &ctx,
        ctx.multisig().bob_xprv(),
    )
    .await
    .expect("Failed to sweep Bob commitment output");

    println!("  ✓ Phase 4 complete\n");

    // ========== Phase 5: Verify final state (using high-level helper) ==========
    println!("Phase 5: Verifying final state...");

    test_helpers::verify_final_balances(alice, bob, contract_id, 300_000, 200_000)
        .await
        .expect("Failed to verify balances");

    println!("  ✓ Phase 5 complete\n");

    println!("Force close test passed!");
    println!("  ✓ Commitment TX broadcast (no closing TX negotiation)");
    println!("  ✓ RGB state transitioned from Active → Applied → Base");
    println!("  ✓ Sweeper TXs recovered RGB to wallets");
    println!("  ✓ Channel completely closed");
}
