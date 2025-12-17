// Async wallet utilities for Lightning RGB tests
//
// Provides helper functions to create and manage LightningRGBWallet instances for testing

use anyhow::{Context, Result};
use bitcoin;
use bpstd::Network;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot; // For Network conversion

// Re-export WalletController for convenience
use lightning_rgb_core::rt_builder::PersistenceStrategy;
pub use lightning_rgb_wallet::wallet::actor::WalletController;
use lightning_rgb_wallet::wallet::{
    builder::{StockpileTxo, WalletBuilder},
    start_wallet_actor,
};

// Re-export for test usage
pub use bitcoin::bip32::Xpriv;

/// Async version of start_rgb_wallet_actor for use in async contexts
/// Returns only the WalletController (shutdown is managed via controller.shutdown())
pub async fn start_rgb_wallet_actor_async(
    storage_dir: PathBuf,
    network: Network,
    indexer_url: String,
    xprv: Xpriv,
    wallet_id: String,
) -> Result<Arc<WalletController>> {
    let (init_tx, init_rx) = oneshot::channel::<Result<WalletController>>();

    // Clone variables for thread
    let thread_storage_dir = storage_dir.clone();
    let thread_network = network;
    let thread_indexer_url = indexer_url;
    let thread_xprv = xprv;
    let thread_wallet_id = wallet_id.clone();

    // Spawn dedicated thread for wallet actor (detached - managed via shutdown signal)
    std::thread::spawn(move || {
        // Create Tokio current_thread runtime
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                let _ = init_tx.send(Err(anyhow::anyhow!("Failed to build Tokio runtime: {}", e)));
                return;
            }
        };

        let local_set = tokio::task::LocalSet::new();

        let main_future = async move {
            // Create shutdown channel
            let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

            let actor_init_result = async {
                // Derive wallet key material from xprv
                let mut keys =
                    lightning_rgb_wallet::utils::key_utils::derive_wallet_key_material_from_xprv(
                        thread_xprv,
                    );
                let signer = keys.get_signer();

                // Build LightningRGBWallet
                let wallet_builder = WalletBuilder::new()
                    .with_id(thread_wallet_id)
                    .with_network(thread_network.to_string().parse().unwrap())
                    .with_persistence_strategy(PersistenceStrategy::Filesystem {
                        base_dir: thread_storage_dir,
                    })
                    .with_rgb_descriptor_config(keys.descriptor.clone())
                    .with_signer(signer)
                    .with_custom_indexer_url(thread_indexer_url);

                let lrgb_wallet = wallet_builder
                    .build_stockpile::<StockpileTxo>()
                    .map_err(|e| anyhow::anyhow!("Failed to build wallet: {}", e))?;

                // Start wallet actor
                let actor_tx = start_wallet_actor(lrgb_wallet);

                // Create controller with shutdown capability
                let controller = WalletController::new_with_shutdown(actor_tx, shutdown_tx);

                Ok(controller)
            }
            .await;

            if init_tx.send(actor_init_result).is_err() {
                eprintln!("Failed to send wallet controller to main thread");
                return;
            }

            // Keep the actor running until shutdown signal
            // IMPORTANT: We must await the shutdown_rx, not use select!
            // The actor task is running in the LocalSet and will be polled automatically
            let _ = shutdown_rx.await;
            println!("Wallet actor received shutdown signal, exiting...");

            // Give actor time to finish processing pending messages
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            println!("Wallet actor main_future completed");
        };

        local_set.block_on(&rt, main_future);
        println!("Wallet actor thread exiting");
    });

    // Wait for initialization asynchronously
    let controller = init_rx
        .await
        .context("Failed to receive from actor thread")??;

    Ok(Arc::new(controller))
}

/// Internal helper: import all issuer files to a wallet
async fn import_issuers_to_wallet(controller: &Arc<WalletController>) -> Result<()> {
    use std::path::PathBuf;

    let issuer_dir = PathBuf::from("tests/templates/schemata");
    if !issuer_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&issuer_dir).context("Failed to read issuer directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "issuer") {
            match rgb::Issuer::load(&path, |_, _, _| Ok::<(), std::io::Error>(())) {
                Ok(issuer) => {
                    if let Err(err) = controller.import_issuer(issuer).await {
                        println!(
                            "Warning: Failed to import issuer {}: {}",
                            path.display(),
                            err
                        );
                    }
                }
                Err(err) => {
                    println!(
                        "Warning: Failed to load issuer file {}: {}",
                        path.display(),
                        err
                    );
                }
            }
        }
    }

    Ok(())
}

/// Internal helper: create wallet with given directory and xprv
async fn create_wallet_with_params(
    wallet_name: &str,
    wallet_dir: PathBuf,
    network: Network,
    indexer_url: String,
    xprv: Xpriv,
) -> Result<Arc<WalletController>> {
    std::fs::create_dir_all(&wallet_dir)
        .context("Failed to create wallet dir")?;

    // Set LRGB_L2_STATE_DIR to wallet directory to isolate L2 state files
    // This prevents state pollution between test runs
    std::env::set_var("LRGB_L2_STATE_DIR", &wallet_dir);

    let controller = start_rgb_wallet_actor_async(
        wallet_dir,
        network,
        indexer_url,
        xprv,
        wallet_name.to_string(),
    )
    .await?;

    import_issuers_to_wallet(&controller).await?;

    Ok(controller)
}

/// Helper function to create a test wallet
///
/// Creates a new wallet with unique directory (timestamp-based).
/// Each call creates a fresh wallet with random keys.
///
/// Note: Wallet shutdown is managed via `controller.shutdown()`, no JoinHandle returned
pub async fn create_test_wallet(
    wallet_name: &str,
    network: Network,
    indexer_url: String,
) -> Result<Arc<WalletController>> {
    let wallet_dir = get_test_wallet_dir(wallet_name);
    let xprv = generate_test_xprv(network);
    create_wallet_with_params(wallet_name, wallet_dir, network, indexer_url, xprv).await
}

/// Create a test wallet with provided xprv
///
/// Similar to create_test_wallet, but uses the provided xprv instead of generating random keys.
/// Useful for multisig testing where reproducible keys are needed.
pub async fn create_wallet_with_xprv(
    wallet_name: &str,
    network: Network,
    indexer_url: String,
    xprv: Xpriv,
) -> Result<Arc<WalletController>> {
    let wallet_dir = get_test_wallet_dir(wallet_name);
    create_wallet_with_params(wallet_name, wallet_dir, network, indexer_url, xprv).await
}

/// Load or create persisted xprv for a wallet
fn load_or_create_xprv(wallet_dir: &PathBuf, network: Network) -> Result<Xpriv> {
    use std::fs;
    use std::io::{Read, Write};

    let xprv_file = wallet_dir.join(".test_xprv");

    // Try to load existing xprv
    if xprv_file.exists() {
        let mut file = fs::File::open(&xprv_file)
            .context("Failed to open xprv file")?;
        let mut hex_str = String::new();
        file.read_to_string(&mut hex_str)
            .context("Failed to read xprv file")?;

        // Parse xprv from hex
        let bytes = hex::decode(hex_str.trim())
            .context("Failed to decode xprv hex")?;
        if bytes.len() != 78 {
            anyhow::bail!("Invalid xprv length");
        }

        Xpriv::decode(&bytes).context("Failed to decode xprv")
    } else {
        // Generate new xprv and save
        let xprv = generate_test_xprv(network);

        let encoded = xprv.encode();
        let hex_str = hex::encode(&encoded);

        let mut file = fs::File::create(&xprv_file)
            .context("Failed to create xprv file")?;
        file.write_all(hex_str.as_bytes())
            .context("Failed to write xprv file")?;

        Ok(xprv)
    }
}

/// Create or reuse a test wallet with fixed directory (for reload testing)
///
/// Each call creates a NEW wallet instance but uses the SAME storage directory.
/// This allows testing true wallet reload scenarios:
/// - First call: creates new wallet, generates and saves xprv
/// - Subsequent calls: loads existing wallet data and xprv from disk
///
/// Note: Does NOT clean up old data - will attempt to load existing wallet state
pub async fn reuse_test_wallet(
    wallet_name: &str,
    network: Network,
    indexer_url: String,
) -> Result<Arc<WalletController>> {
    let wallet_dir = get_test_wallet_dir_fixed(wallet_name);
    std::fs::create_dir_all(&wallet_dir)
        .context("Failed to create wallet dir")?;

    // Clean up delta directory to ensure fresh reload
    // Delta layer contains temporary state from previous run that should be discarded
    let delta_dir = wallet_dir
        .join("bitcoin")
        .join(network.to_string().to_lowercase())
        .join("delta");
    if delta_dir.exists() {
        std::fs::remove_dir_all(&delta_dir)
            .context("Failed to remove delta dir")?;
    }

    // Load or create persisted xprv
    let xprv = load_or_create_xprv(&wallet_dir, network)?;

    create_wallet_with_params(wallet_name, wallet_dir, network, indexer_url, xprv).await
}

/// Generate a random test xpriv for the given network
///
/// Uses random seed to ensure each wallet has unique keys.
/// WARNING: This is for testing only! Do not use in production.
fn generate_test_xprv(network: Network) -> Xpriv {
    use bitcoin::bip32::Xpriv;
    use rand::RngCore;

    let mut seed = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut seed);

    // Convert bpstd::Network to bitcoin::Network
    let bitcoin_network: bitcoin::Network = match network {
        Network::Mainnet => bitcoin::Network::Bitcoin,
        Network::Testnet3 => bitcoin::Network::Testnet,
        Network::Regtest => bitcoin::Network::Regtest,
        Network::Signet => bitcoin::Network::Signet,
        _ => bitcoin::Network::Regtest,
    };

    Xpriv::new_master(bitcoin_network, &seed).expect("Failed to create master key")
}

/// Get test wallet directory with unique timestamp (for isolated tests)
pub fn get_test_wallet_dir_unique(wallet_name: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    PathBuf::from("tests")
        .join("test_wallets")
        .join("unique")
        .join(format!("{}_{}", wallet_name, timestamp))
}

/// Get test wallet directory with fixed name (for reload tests)
pub fn get_test_wallet_dir_fixed(wallet_name: &str) -> PathBuf {
    PathBuf::from("tests")
        .join("test_wallets")
        .join("fixed")
        .join(wallet_name)
}

/// Get test wallet directory under tests/test_wallets
/// Uses a fixed directory instead of temp for easier debugging
///
/// DEPRECATED: Use get_test_wallet_dir_unique() or get_test_wallet_dir_fixed() instead
pub fn get_test_wallet_dir(wallet_name: &str) -> PathBuf {
    // Default to unique mode for backward compatibility
    get_test_wallet_dir_unique(wallet_name)
}

/// Cleanup test wallet data
pub async fn cleanup_test_wallet(wallet_name: &str) -> Result<()> {
    let wallet_dir = get_test_wallet_dir(wallet_name);
    if wallet_dir.exists() {
        std::fs::remove_dir_all(&wallet_dir)
            .context("Failed to cleanup wallet dir")?;
    }
    Ok(())
}
