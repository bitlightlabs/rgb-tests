// 2-2 MultiSig tests for Lightning RGB
//
// RGB funding to multisig address tests

use bpstd::Network;
use ln_rgb_tests::utils::{
    asset_params::NIAIssueParams, chain, multisig::MultiSigHelper, test_helpers::*, wallet::*, *,
};
use lightning_rgb_ln_types::{AssetId, AssetQuantity};
use lightning_rgb_wallet::wallet::RgbFundingRequest;
use serial_test::serial;
use std::str::FromStr;

/// Test RGB funding to 2-2 multisig address
///
/// Flow:
/// 1. Create Alice and Bob wallets with fixed xprv
/// 2. Alice issues NIA contract
/// 3. Share contract with Bob
/// 4. Create 2-2 multisig address
/// 5. Alice creates funding PSBT (transfers RGB to multisig)
/// 6. Alice signs and broadcasts funding TX
/// 7. Bob accepts RGB transfer
/// 8. Verify RGB assets on multisig UTXO
#[tokio::test]
#[serial]
async fn test_rgb_funding_to_multisig() {
    println!("\n=== Test: RGB Funding to 2-2 Multisig ===\n");

    // Initialize
    chain::initialize().await;
    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallets with fixed keys
    println!("Creating wallets with fixed keys...");
    let alice_xprv = generate_fixed_xpriv("alice");
    let bob_xprv = generate_fixed_xpriv("bob");

    let alice = create_wallet_with_xprv("multisig_alice", network, indexer_url.clone(), alice_xprv)
        .await
        .expect("Failed to create Alice wallet");

    let bob = create_wallet_with_xprv("multisig_bob", network, indexer_url, bob_xprv)
        .await
        .expect("Failed to create Bob wallet");

    println!("✓ Wallets created");

    // Step 1: Alice issues NIA contract
    println!("\nStep 1: Alice issues NIA contract...");
    // Fund Alice with enough BTC - this UTXO will hold the RGB allocation
    let issue_utxo = get_utxo(&alice, Some(200_000), INSTANCE_1)
        .await
        .expect("Failed to get issue UTXO");

    let mut params = NIAIssueParams::new("TestMultisigNIA", "TMNIA", "centiMilli", 1_000_000);
    params.add_allocation(issue_utxo, 1_000_000);

    let contract_id = alice
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue NIA");

    println!("✓ NIA contract issued: {}", contract_id);
    println!("  Total supply: 1,000,000 TMNIA");

    // Verify Alice's allocation
    let alice_allocations = alice
        .get_allocations(contract_id)
        .await
        .expect("Failed to get allocations");
    println!("  Alice initial allocations: {}", alice_allocations.len());

    // Step 2: Share contract with Bob
    println!("\nStep 2: Sharing contract with Bob...");
    share_contract(&alice, &bob, contract_id, "TestMultisigNIA")
        .await
        .expect("Failed to share contract");
    println!("✓ Contract shared with Bob");

    // Step 3: Create 2-2 multisig address
    println!("\nStep 3: Creating multisig address...");
    let multisig = MultiSigHelper::new(alice_xprv, bob_xprv);
    let multisig_script = multisig.create_2of2_script();
    let multisig_addr = multisig.create_multisig_address(bitcoin::Network::Regtest);
    println!("✓ Multisig address: {}", multisig_addr);
    println!("  Script: {} bytes", multisig_script.len());

    // Step 4: Alice creates funding PSBT
    println!("\nStep 4: Creating funding PSBT...");
    let channel_value_sats = 100_000;
    let rgb_amount = 500_000; // Transfer 500k TMNIA to multisig

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
        .expect("Failed to create funding PSBT");

    println!("✓ Funding PSBT created");
    println!("  RGB amount: {} TMNIA → multisig", rgb_amount);
    println!("  Bitcoin amount: {} sats → multisig", channel_value_sats);

    // Step 5: Alice signs and broadcasts
    println!("\nStep 5: Signing and broadcasting funding transaction...");

    // Convert to bp PSBT
    use lightning_rgb_types::{convert_psbt_from_bitcoin_to_bp, convert_psbt_from_bp_to_bitcoin};
    let bp_psbt = convert_psbt_from_bitcoin_to_bp(&funding_result.psbt)
        .expect("Failed to convert PSBT to bp format");

    // Sign with wallet
    let signed_bp_psbt = alice.sign_finalize(bp_psbt).await;

    // Convert back and extract TX
    let signed_bitcoin_psbt = convert_psbt_from_bp_to_bitcoin(&signed_bp_psbt);
    let signed_funding_tx = signed_bitcoin_psbt
        .extract_tx()
        .expect("Failed to extract transaction from PSBT");

    println!("  ✓ Transaction signed");
    println!("  Witness stack size: {}", signed_funding_tx.input[0].witness.len());

    // Broadcast
    let funding_txid = broadcast_and_mine(&signed_funding_tx, 1, INSTANCE_1, network)
        .expect("Failed to broadcast funding transaction");

    println!("  ✓ Funding transaction confirmed: {}", funding_txid);

    // Step 6: Bob accepts RGB transfer
    println!("\nStep 6: Bob accepting RGB transfer...");
    let consignment_key = funding_txid.clone();

    bob.accept_transfer(consignment_key)
        .await
        .expect("Failed to accept transfer");

    println!("✓ Bob accepted RGB transfer");

    // Step 7: Verify RGB assets
    println!("\nStep 7: Verifying RGB asset allocations...");

    // Sync wallets
    alice.sync_runtime().await.expect("Failed to sync Alice");
    bob.sync_runtime().await.expect("Failed to sync Bob");

    // Check Alice's remaining balance
    check_allocations_sum(&alice, contract_id, 500_000)
        .await
        .expect("Alice should have 500k TMNIA remaining");

    println!("  ✓ Alice: 500,000 TMNIA remaining");

    // Check Bob's view of the contract - using get_allocations (wallet-owned only)
    let bob_allocations = bob
        .get_allocations(contract_id)
        .await
        .expect("Failed to get Bob's allocations");

    println!("  Bob sees {} wallet-owned allocations", bob_allocations.len());

    // Check Bob's full contract state view (including multisig)
    println!("\n  Checking full contract state (including multisig)...");
    let bob_full_state = bob
        .get_contract_state_full(contract_id)
        .await
        .expect("Failed to get full contract state");

    // Print all owned states
    if let Some(balance_states) = bob_full_state.owned.get("balance") {
        println!("  Total balance states: {}", balance_states.len());
        for (idx, state) in balance_states.iter().enumerate() {
            println!("    State {}: outpoint={}, amount={}, status={:?}",
                idx,
                state.assignment.seal,
                state.assignment.data.unwrap_num().unwrap_uint::<u64>(),
                state.status
            );
        }

        // Step: Find multisig vout by script_pubkey
        let multisig_script_pubkey = multisig_addr.script_pubkey();
        let multisig_vout = signed_funding_tx
            .output
            .iter()
            .position(|output| output.script_pubkey == multisig_script_pubkey)
            .expect("Multisig output not found in funding transaction");

        println!("\n  ✓ Found multisig output at vout {}", multisig_vout);

        // Construct the expected txid:vout seal
        let funding_txid_parsed = bitcoin::Txid::from_str(&funding_txid)
            .expect("Failed to parse funding txid");
        let expected_seal_prefix = format!("{}:{}", funding_txid_parsed, multisig_vout);

        println!("  Looking for RGB state with seal: {}/*", expected_seal_prefix);

        // Find RGB state matching the multisig UTXO
        let multisig_state = balance_states
            .iter()
            .find(|state| {
                let seal_str = state.assignment.seal.to_string();
                seal_str.starts_with(&expected_seal_prefix)
            })
            .expect("Multisig UTXO state not found in RGB contract");

        // Verify the state
        let multisig_amount = multisig_state.assignment.data.unwrap_num().unwrap_uint::<u64>();
        assert_eq!(multisig_amount, rgb_amount, "Multisig RGB amount mismatch");
        assert!(multisig_state.status.is_valid(), "Multisig state should be valid");

        println!("  ✓ Multisig RGB state verified:");
        println!("    Seal: {}", multisig_state.assignment.seal);
        println!("    Amount: {} TMNIA", multisig_amount);
        println!("    Status: {:?}", multisig_state.status);
    } else {
        panic!("No balance states found in full contract state");
    }

    // Note: Bob accepted the consignment, so he knows about the RGB assets on multisig UTXO
    // But he cannot spend them alone (needs 2-2 signature)

    println!("\n=== Test Complete ===");
    println!(" RGB Funding to Multisig:");
    println!("  • Alice issued 1,000,000 TMNIA");
    println!("  • Contract shared with Bob");
    println!("  • Multisig address created (P2WSH)");
    println!("  • Alice transferred 500,000 TMNIA to multisig");
    println!("  • Bob accepted the transfer");
    println!("  • Alice retains 500,000 TMNIA");
    println!("  • Bob sees the RGB assets (but cannot spend alone)");
    println!("\n All components working correctly!");

    // Cleanup
    cleanup_test_wallet("multisig_alice").await.ok();
    cleanup_test_wallet("multisig_bob").await.ok();
}

/// Test spending RGB from multisig UTXO
///
/// Flow:
/// 1. Continue from previous test (multisig UTXO with 500k TMNIA)
/// 2. Create spending TX: multisig → Alice's addr (250k) + Bob's addr (250k)
/// 3. Add RGB coloring to the TX
/// 4. Sign with 2-2 multisig
/// 5. Broadcast and verify RGB distribution
#[tokio::test]
#[serial]
async fn test_spend_multisig_with_rgb() {
    println!("\n=== Test: Spend Multisig UTXO with RGB ===\n");

    // Initialize
    chain::initialize().await;
    let network = Network::Regtest;
    let indexer_url = chain::indexer_url(INSTANCE_1, network);

    // Create wallets with fixed keys (same as funding test)
    println!("Creating wallets with fixed keys...");
    let alice_xprv = generate_fixed_xpriv("alice");
    let bob_xprv = generate_fixed_xpriv("bob");

    let alice = create_wallet_with_xprv("multisig_spend_alice", network, indexer_url.clone(), alice_xprv)
        .await
        .expect("Failed to create Alice wallet");

    let bob = create_wallet_with_xprv("multisig_spend_bob", network, indexer_url, bob_xprv)
        .await
        .expect("Failed to create Bob wallet");

    println!("✓ Wallets created\n");

    // Step 1: Setup - Issue contract and fund multisig (same as previous test)
    println!("Step 1: Setting up multisig with RGB assets...");

    // Issue NIA contract
    let issue_utxo = get_utxo(&alice, Some(200_000), INSTANCE_1)
        .await
        .expect("Failed to get issue UTXO");

    let mut params = NIAIssueParams::new("TestSpendNIA", "TSNIA", "centiMilli", 1_000_000);
    params.add_allocation(issue_utxo, 1_000_000);

    let contract_id = alice
        .issue(params.into_create_params())
        .await
        .expect("Failed to issue NIA");

    println!("  ✓ Issued contract: {}", contract_id);

    // Share contract with Bob
    share_contract(&alice, &bob, contract_id, "TestSpendNIA")
        .await
        .expect("Failed to share contract");

    // Create multisig address
    let multisig = MultiSigHelper::new(alice_xprv, bob_xprv);
    let multisig_script = multisig.create_2of2_script();
    let multisig_addr = multisig.create_multisig_address(bitcoin::Network::Regtest);
    println!("  ✓ Multisig address: {}", multisig_addr);

    // Fund multisig with RGB
    let channel_value_sats = 100_000;
    let rgb_amount = 500_000;

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
        .expect("Failed to create funding PSBT");

    // Sign and broadcast funding TX
    use lightning_rgb_types::{convert_psbt_from_bitcoin_to_bp, convert_psbt_from_bp_to_bitcoin};
    let bp_psbt = convert_psbt_from_bitcoin_to_bp(&funding_result.psbt)
        .expect("Failed to convert PSBT to bp format");

    let signed_bp_psbt = alice.sign_finalize(bp_psbt).await;
    let signed_bitcoin_psbt = convert_psbt_from_bp_to_bitcoin(&signed_bp_psbt);
    let signed_funding_tx = signed_bitcoin_psbt
        .extract_tx()
        .expect("Failed to extract transaction from PSBT");

    let funding_txid = broadcast_and_mine(&signed_funding_tx, 1, INSTANCE_1, network)
        .expect("Failed to broadcast funding transaction");

    println!("  ✓ Funding TX confirmed: {}", funding_txid);

    // Bob accepts transfer
    bob.accept_transfer(funding_txid.clone())
        .await
        .expect("Failed to accept transfer");

    // Sync wallets
    alice.sync_runtime().await.expect("Failed to sync Alice");
    bob.sync_runtime().await.expect("Failed to sync Bob");

    println!("  ✓ Multisig UTXO funded with 500,000 TSNIA\n");

    // Step 2: Find multisig UTXO
    println!("Step 2: Locating multisig UTXO...");

    let multisig_script_pubkey = multisig_addr.script_pubkey();
    let multisig_vout = signed_funding_tx
        .output
        .iter()
        .position(|output| output.script_pubkey == multisig_script_pubkey)
        .expect("Multisig output not found");

    let multisig_outpoint = bitcoin::OutPoint {
        txid: bitcoin::Txid::from_str(&funding_txid).expect("Failed to parse txid"),
        vout: multisig_vout as u32,
    };

    let multisig_txout = signed_funding_tx.output[multisig_vout].clone();

    println!("  ✓ Multisig UTXO: {}:{}", multisig_outpoint.txid, multisig_outpoint.vout);
    println!("  Value: {} sats", multisig_txout.value.to_sat());
    println!("  RGB: 500,000 TSNIA\n");

    // Step 3: Create destination addresses
    println!("Step 3: Creating destination addresses...");

    let alice_dest_addr = alice.get_address().await.expect("Failed to get Alice address");
    let bob_dest_addr = bob.get_address().await.expect("Failed to get Bob address");

    println!("  Alice destination: {}", alice_dest_addr);
    println!("  Bob destination: {}\n", bob_dest_addr);

    // Step 4: Create spending transaction (Bitcoin layer)
    println!("Step 4: Creating spending transaction...");

    let fee_sats = 1_000;
    let total_output_sats = multisig_txout.value.to_sat() - fee_sats;
    let alice_sats = total_output_sats / 2;
    let bob_sats = total_output_sats - alice_sats;

    let spending_tx = bitcoin::Transaction {
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
                value: bitcoin::Amount::from_sat(alice_sats),
                script_pubkey: bitcoin::ScriptBuf::from_bytes(alice_dest_addr.script_pubkey().to_vec()),
            },
            bitcoin::TxOut {
                value: bitcoin::Amount::from_sat(bob_sats),
                script_pubkey: bitcoin::ScriptBuf::from_bytes(bob_dest_addr.script_pubkey().to_vec()),
            },
        ],
    };

    println!("  ✓ Spending TX created");
    println!("    Input: {} sats from multisig", multisig_txout.value.to_sat());
    println!("    Output 0 (Alice): {} sats", alice_sats);
    println!("    Output 1 (Bob): {} sats\n", bob_sats);

    // Step 5: Add RGB coloring
    println!("Step 5: Adding RGB coloring to transaction...");

    use lightning_rgb_types::{AssetColoringInfo, ColoringInfo, PlainOutpoint, AssetIface, contract_id_from_bytes};
    use std::collections::HashMap;

    // Split RGB equally: 250k each
    let alice_rgb_share = 250_000;
    let bob_rgb_share = 250_000;

    // Define vouts for Alice and Bob based on transaction outputs
    let alice_vout: u32 = 0;  // Alice's output is vout 0
    let bob_vout: u32 = 1;    // Bob's output is vout 1

    let asset_coloring_info = AssetColoringInfo {
        iface: AssetIface::RGB20,
        input_outpoints: vec![PlainOutpoint {
            txid: multisig_outpoint.txid.to_string(),
            vout: multisig_outpoint.vout,
        }],
        output_map: HashMap::from([
            (alice_vout, alice_rgb_share),
            (bob_vout, bob_rgb_share),
        ]),
        static_blinding: None,
    };

    let coloring_info = ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_from_bytes(AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes()).unwrap(),
            asset_coloring_info,
        )]),
        static_blinding: None,
        nonce: None,
    };

    println!("  RGB allocation:");
    println!("    Alice (vout {}): {} TSNIA", alice_vout, alice_rgb_share);
    println!("    Bob (vout {}): {} TSNIA", bob_vout, bob_rgb_share);

    // Convert to PSBT
    let mut psbt = bitcoin::Psbt::from_unsigned_tx(spending_tx.clone())
        .expect("Failed to create PSBT");

    // Add witness_utxo (required for RGB coloring)
    psbt.inputs[0].witness_utxo = Some(multisig_txout.clone());

    // Color the PSBT
    let colored_psbt = alice
        .color_psbt(psbt, coloring_info)
        .await
        .expect("Failed to color PSBT");

    println!("  ✓ PSBT colored with RGB commitments");
    println!("  ✓ RGB state saved to pending_commitments[{}]\n", multisig_outpoint.txid);

    // Step 6: Verify Alice's Pending State
    println!("Step 6: Verifying Alice's Pending RGB state...");

    use lightning_rgb_types::RgbStateLayer;

    let alice_pending_allocations = alice
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Pending)
        .await
        .expect("Failed to get Alice pending allocations");

    println!("  Alice pending allocations: {} entries", alice_pending_allocations.len());
    for (outpoint, amount) in &alice_pending_allocations {
        println!("    - {}:{} = {} TSNIA", outpoint.txid, outpoint.vout_u32(), amount);
    }

    // Verify Alice's 250k allocation exists in pending
    let spending_tx_txid = colored_psbt.unsigned_tx.compute_txid();
    let alice_pending_amount: u64 = alice_pending_allocations
        .iter()
        .filter(|(op, _)| {
            use lightning_rgb_types::convert_txid_from_bitcoin_to_bp;
            op.txid == convert_txid_from_bitcoin_to_bp(spending_tx_txid) && op.vout_u32() == alice_vout
        })
        .map(|(_, amt)| amt)
        .sum();

    println!("  ✓ Alice's pending allocation (vout {}): {} TSNIA", alice_vout, alice_pending_amount);
    assert_eq!(alice_pending_amount, alice_rgb_share, "Alice should have {} in pending state", alice_rgb_share);

    // Step 7: Sign with 2-2 multisig (NOT broadcast!)
    println!("\nStep 7: Signing with 2-2 multisig (NOT broadcasting)...");

    let mut signed_tx = colored_psbt.extract_tx_unchecked_fee_rate();

    multisig.finalize_tx(&mut signed_tx, &multisig_script, multisig_txout.value)
        .expect("Failed to sign with multisig");

    println!("  ✓ Transaction signed by Alice and Bob");
    println!("    NOT broadcasting (mimicking Lightning commitment TX)\n");

    // Step 8: Alice ACK (promote pending → active)
    println!("Step 8: Alice acknowledging commitment (pending → active)...");

    // Use the funding txid directly (bitcoin::Txid type)
    let multisig_funding_txid = multisig_outpoint.txid;

    alice
        .on_revoke_and_ack_by_commitment(multisig_funding_txid)
        .await
        .expect("Failed to promote Alice's state to active");

    println!("  ✓ Alice's state promoted: pending → active\n");

    // Step 9: Verify Alice's Active State
    println!("Step 9: Verifying Alice's Active RGB state...");

    let alice_active_allocations = alice
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Active)
        .await
        .expect("Failed to get Alice active allocations");

    println!("  Alice active allocations: {} entries", alice_active_allocations.len());
    for (outpoint, amount) in &alice_active_allocations {
        println!("    - {}:{} = {} TSNIA", outpoint.txid, outpoint.vout_u32(), amount);
    }

    // Verify Alice's 250k allocation exists in active
    let alice_active_amount: u64 = alice_active_allocations
        .iter()
        .filter(|(op, _)| {
            use lightning_rgb_types::convert_txid_from_bitcoin_to_bp;
            op.txid == convert_txid_from_bitcoin_to_bp(spending_tx_txid) && op.vout_u32() == 0
        })
        .map(|(_, amt)| amt)
        .sum();

    println!("  ✓ Alice's active allocation (vout 0): {} TSNIA", alice_active_amount);
    assert_eq!(alice_active_amount, 250_000, "Alice should have 250k in active state");

    // Step 10: Bob's symmetric flow
    println!("\nStep 10: Bob's Lightning RGB flow (symmetric to Alice)...");

    // Bob colors the SAME spending TX with SAME RGB allocations
    println!("  Bob coloring spending TX...");

    // Bob uses the same ColoringInfo as Alice
    let bob_asset_coloring_info = AssetColoringInfo {
        iface: AssetIface::RGB20,
        input_outpoints: vec![PlainOutpoint {
            txid: multisig_outpoint.txid.to_string(),
            vout: multisig_outpoint.vout,
        }],
        output_map: HashMap::from([
            (alice_vout, alice_rgb_share),
            (bob_vout, bob_rgb_share),
        ]),
        static_blinding: None,
    };

    let bob_coloring_info = ColoringInfo {
        asset_info_map: HashMap::from_iter([(
            contract_id_from_bytes(AssetId::from(contract_id.to_byte_array().as_slice()).as_bytes()).unwrap(),
            bob_asset_coloring_info,
        )]),
        static_blinding: None,
        nonce: None,
    };

    // Bob creates his own PSBT from the same spending_tx
    let mut bob_psbt = bitcoin::Psbt::from_unsigned_tx(spending_tx.clone())
        .expect("Failed to create PSBT for Bob");
    bob_psbt.inputs[0].witness_utxo = Some(multisig_txout.clone());

    let bob_colored_psbt = bob
        .color_psbt(
            bob_psbt,
            bob_coloring_info,
        )
        .await
        .expect("Failed to color PSBT for Bob");

    println!("  ✓ Bob's PSBT colored with RGB commitments\n");

    // Step 11: Verify Bob's Pending State
    println!("Step 11: Verifying Bob's Pending RGB state...");

    let bob_pending_allocations = bob
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Pending)
        .await
        .expect("Failed to get Bob pending allocations");

    println!("  Bob pending allocations: {} entries", bob_pending_allocations.len());
    for (outpoint, amount) in &bob_pending_allocations {
        println!("    - {}:{} = {} TSNIA", outpoint.txid, outpoint.vout_u32(), amount);
    }

    // Verify Bob's 250k allocation exists in pending (Bob's vout)
    let bob_spending_tx_txid = bob_colored_psbt.unsigned_tx.compute_txid();
    let bob_pending_amount: u64 = bob_pending_allocations
        .iter()
        .filter(|(op, _)| {
            use lightning_rgb_types::convert_txid_from_bitcoin_to_bp;
            op.txid == convert_txid_from_bitcoin_to_bp(bob_spending_tx_txid) && op.vout_u32() == bob_vout
        })
        .map(|(_, amt)| amt)
        .sum();

    println!("  ✓ Bob's pending allocation (vout {}): {} TSNIA", bob_vout, bob_pending_amount);
    assert_eq!(bob_pending_amount, bob_rgb_share, "Bob should have {} in pending state", bob_rgb_share);

    // Step 12: Bob ACK (promote pending → active)
    println!("\nStep 12: Bob acknowledging commitment (pending → active)...");

    bob
        .on_revoke_and_ack_by_commitment(multisig_funding_txid)
        .await
        .expect("Failed to promote Bob's state to active");

    println!("  ✓ Bob's state promoted: pending → active\n");

    // Step 13: Verify Bob's Active State
    println!("Step 13: Verifying Bob's Active RGB state...");

    let bob_active_allocations = bob
        .get_rgb_allocations_in_layer(contract_id, RgbStateLayer::Active)
        .await
        .expect("Failed to get Bob active allocations");

    println!("  Bob active allocations: {} entries", bob_active_allocations.len());
    for (outpoint, amount) in &bob_active_allocations {
        println!("    - {}:{} = {} TSNIA", outpoint.txid, outpoint.vout_u32(), amount);
    }

    // Verify Bob's 250k allocation exists in active
    let bob_active_amount: u64 = bob_active_allocations
        .iter()
        .filter(|(op, _)| {
            use lightning_rgb_types::convert_txid_from_bitcoin_to_bp;
            op.txid == convert_txid_from_bitcoin_to_bp(bob_spending_tx_txid) && op.vout_u32() == bob_vout
        })
        .map(|(_, amt)| amt)
        .sum();

    println!("  ✓ Bob's active allocation (vout {}): {} TSNIA", bob_vout, bob_active_amount);
    assert_eq!(bob_active_amount, bob_rgb_share, "Bob should have {} in active state", bob_rgb_share);

    println!("\n=== Test Complete ===");
    println!(" Lightning-style Multisig RGB Flow (Alice & Bob):");
    println!("  • Created multisig UTXO with 500,000 TSNIA");
    println!("  • Alice colored spending TX → Pending → Active (250k TSNIA)");
    println!("  • Bob colored spending TX → Pending → Active (250k TSNIA)");
    println!("  • 2-2 multisig signatures added");
    println!("  • Both parties verified their Active layer allocations");
    println!("  •   TX NOT broadcasted (mimicking LN commitment)");
    println!("\n Both Alice and Bob's Lightning RGB flows verified!");

    // Cleanup
    cleanup_test_wallet("multisig_spend_alice").await.ok();
    cleanup_test_wallet("multisig_spend_bob").await.ok();
}
