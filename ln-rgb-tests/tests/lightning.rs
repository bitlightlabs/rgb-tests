// //! P0 - RGB State Query for Off-chain Witness UTXOs
// //!
// //! **Core Problem**
// //! In Lightning Network, Commitment TX and HTLC TX exist only off-chain and are not
// //! broadcasted to mempool. However, the receiver needs to query RGB state on these
// //! witness UTXOs to verify:
// //! 1. Whether Commitment TX's RGB balance is correct
// //! 2. Whether HTLC output contains the correct RGB assets
// //!
// //! **Current Issue**
// //! rgb-std's state_own() method depends on wallet.has_utxo() check and only returns
// //! UTXOs already in the wallet. In original rgb-tests, accept_transfer() calls sync()
// //! to fetch unconfirmed transactions from mempool and register witness UTXOs to wallet.
// //! This doesn't work in Lightning scenarios.
// //!
// //! **Solution Exploration**
// //! Possible approaches:
// //! 1. Manual PSBT registration: Call wallet.register_psbt() before accept_transfer to
// //!    register off-chain transactions
// //! 2. Extend state_own(): Support querying RGB state for UTXOs not in wallet (tentative allocations)
// //! 3. Self-contained Consignment: Consignment carries sufficient information without
// //!    depending on wallet UTXO list
// //!
// //! **Testing Goals**
// //! Verify that receiver can, without syncing wallet:
// //! 1. Accept RGB consignment containing witness UTXO
// //! 2. Query RGB state on witness UTXO
// //! 3. Verify RGB balance correctness
// //!
// //! Related code locations:
// //! - bp-wallet/src/wallet.rs:352 - register_psbt() method
// //! - rgb-std/src/popls/bp.rs:434 - has_utxo() check in state_own()
// //! - rgb-tests/tests/utils/helper/wallet.rs:810 - sync() call in accept_transfer()
// #[test]
// #[ignore = "TODO: Need to implement RGB state query mechanism for off-chain witness UTXOs"]
// fn test_offchain_witness_rgb_query() {
//     // TODO: Implementation steps
//     // 1. Create sender and receiver wallets
//     // 2. Sender issues RGB asset
//     // 3. Create witness transfer (without broadcasting transaction)
//     // 4. Receiver accepts consignment without syncing
//     // 5. Receiver can query RGB state on witness UTXO
//     // 6. Verify RGB balance correctness
//     todo!("Implement RGB state query for off-chain witness UTXOs")
// }

// //! P1 - Complete Four-Phase Flow Test
// //!
// //! Verifies the complete Funding → Commitment → HTLC → Closing flow
// #[test]
// fn test_four_phase_flow() {
//     // TODO: Complete four-phase flow test
//     todo!("Implement complete four-phase flow")
// }

// #[test]
// fn test_commitment_update() {
//     // TODO: Test Commitment update and rollback
//     todo!("Implement Commitment update test")
// }

// //! P0 - HTLC RGB Flow Test
// //!
// //! Verifies correct RGB transfer through HTLC outputs
// #[test]
// fn test_htlc_rgb_flow() {
//     // TODO: Commitment TX with HTLC → HTLC Success TX
//     todo!("Implement HTLC RGB flow test")
// }

// #[test]
// fn test_htlc_conservation() {
//     // TODO: Verify RGB conservation in HTLC transactions
//     todo!("Implement HTLC conservation check")
// }

// #[test]
// fn test_conservation_check() {
//     // TODO: Verify sum(input_rgb) == sum(output_rgb)
//     todo!("Implement RGB conservation check")
// }
