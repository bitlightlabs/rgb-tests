use super::*;
use esplora::api::TxStatus;
use tokio::process::Command as TokioCommand;

static INIT: Once = Once::new();

pub static INDEXER: OnceLock<Indexer> = OnceLock::new();

// Node addresses
const NODE2_ADDR: &str = "172.30.2.205:18444";
const NODE3_ADDR: &str = "172.30.2.206:18444";
#[allow(dead_code)]
const NODE2_IP: &str = "172.30.2.205";
#[allow(dead_code)]
const NODE3_IP: &str = "172.30.2.206";

// Only Esplora for Lightning RGB tests
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub enum Indexer {
    #[default]
    Esplora,
}

impl fmt::Display for Indexer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

pub async fn initialize() {
    // Always use Esplora for Lightning RGB tests
    INDEXER.get_or_init(|| Indexer::Esplora);

    // Use a static flag to track if initialization was already done
    static mut INITIALIZED: bool = false;

    // Check if already initialized
    unsafe {
        if INITIALIZED {
            return;
        }
    }

    INIT.call_once(|| unsafe {
        INITIALIZED = true;

        // Initialize tracing subscriber for internal logging
        // This allows us to see detailed logs from wallet operations
        use tracing_subscriber::{fmt, EnvFilter};

        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

        let filter_str = format!("{:?}", filter);

        fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .init();

        println!("Tracing initialized with filter: {}", filter_str);
    });

    if std::env::var("SKIP_INIT").is_ok() {
        println!("skipping services initialization");
        return;
    }

    // Use parent directory's docker setup (rgb-tests/tests/docker)
    let start_services_file = PathBuf::from("..")
        .join("tests")
        .join("docker")
        .join("start_services.sh");

    if !start_services_file.exists() {
        panic!("start_services.sh not found at {:?}", start_services_file);
    }

    println!("starting test services...");

    let start_output = TokioCommand::new(&start_services_file)
        .arg("start")
        .output()
        .await
        .expect("failed to start test services");

    if !start_output.status.success() {
        println!("stdout: {}", String::from_utf8_lossy(&start_output.stdout));
        println!("stderr: {}", String::from_utf8_lossy(&start_output.stderr));
        panic!("failed to start test services");
    }

    // Wait for all nodes to be ready
    for instance in INSTANCE_1..=INSTANCE_3 {
        let mut attempts = 0;
        let max_attempts = 30;

        loop {
            if attempts >= max_attempts {
                panic!("Node {instance} failed to start after {max_attempts} attempts");
            }

            let result = _bitcoin_cli_cmd_async(instance, vec!["getblockchaininfo"]).await;
            if !result.is_empty() {
                break;
            }

            attempts += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // Wait for indexer sync
        _wait_indexer_sync_async(instance).await;
    }

    println!("Test services ready");
}

static MINER: Lazy<RwLock<Miner>> = Lazy::new(|| RwLock::new(Miner { no_mine_count: 0 }));

#[derive(Clone, Debug)]
pub struct Miner {
    no_mine_count: u32,
}

// Only Esplora
fn _service_base_name() -> String {
    "esplora".to_string()
}

// Async version of _bitcoin_cli_cmd
async fn _bitcoin_cli_cmd_async(instance: u8, args: Vec<&str>) -> String {
    // Use parent directory's docker setup (rgb-tests/tests/docker)
    let compose_file = PathBuf::from("..")
        .join("tests")
        .join("docker")
        .join("docker-compose.yml");
    let mut bitcoin_cli = vec![
        s!("-p"),
        s!("rgb-tests"),
        s!("-f"),
        compose_file.to_string_lossy().to_string(),
        s!("exec"),
        s!("-T"),
    ];

    let service_name = format!("bitcoin-core-{instance}");
    bitcoin_cli.extend(vec![
        service_name,
        "bitcoin-cli".to_string(),
        "-regtest".to_string(),
    ]);

    let docker_dir = PathBuf::from("..").join("tests").join("docker");
    let output = TokioCommand::new("docker")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&docker_dir)
        .arg("compose")
        .args(bitcoin_cli)
        .args(&args)
        .output()
        .await
        .unwrap_or_else(|_| panic!("failed to call bitcoind with args {args:?}"));

    if !output.status.success() {
        println!("{output:?}");
        panic!("failed to get successful output with args {args:?}");
    }
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

// Keep synchronous version for compatibility
fn _bitcoin_cli_cmd(instance: u8, args: Vec<&str>) -> String {
    let compose_file = PathBuf::from("..")
        .join("tests")
        .join("docker")
        .join("docker-compose.yml");
    let mut bitcoin_cli = vec![
        s!("-p"),
        s!("rgb-tests"),
        s!("-f"),
        compose_file.to_string_lossy().to_string(),
        s!("exec"),
        s!("-T"),
    ];

    let service_name = format!("bitcoin-core-{instance}");
    bitcoin_cli.extend(vec![
        service_name,
        "bitcoin-cli".to_string(),
        "-regtest".to_string(),
    ]);

    let docker_dir = PathBuf::from("..").join("tests").join("docker");
    let output = Command::new("docker")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&docker_dir)
        .arg("compose")
        .args(bitcoin_cli)
        .args(&args)
        .output()
        .unwrap_or_else(|_| panic!("failed to call bitcoind with args {args:?}"));

    if !output.status.success() {
        println!("{output:?}");
        panic!("failed to get successful output with args {args:?}");
    }
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

impl Miner {
    fn mine(&self, instance: u8, blocks: u32) -> bool {
        if self.no_mine_count > 0 {
            return false;
        }
        self.force_mine(instance, blocks)
    }

    fn force_mine(&self, instance: u8, blocks: u32) -> bool {
        _bitcoin_cli_cmd(
            instance,
            vec!["-rpcwallet=miner", "-generate", &blocks.to_string()],
        );
        _wait_indexer_sync(instance);
        true
    }

    fn stop_mining(&mut self) {
        self.no_mine_count += 1;
    }

    fn resume_mining(&mut self) {
        if self.no_mine_count > 0 {
            self.no_mine_count -= 1;
        }
    }
}

/// Get current blockchain height
pub async fn get_block_count() -> u64 {
    get_block_count_for_instance(INSTANCE_1).await
}

/// Get blockchain height for a specific instance
pub async fn get_block_count_for_instance(instance: u8) -> u64 {
    let result = _bitcoin_cli_cmd(instance, vec!["getblockcount"]);
    result
        .trim()
        .parse::<u64>()
        .expect("Failed to parse block count")
}

pub fn mine(resume: bool) {
    mine_custom(resume, INSTANCE_1, 1);
}

pub fn mine_custom(resume: bool, instance: u8, blocks: u32) {
    let t_0 = OffsetDateTime::now_utc();
    if resume {
        resume_mining();
    }
    loop {
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 120.0 {
            println!("forcibly breaking mining wait");
            resume_mining();
        }
        let mined = MINER.read().as_ref().unwrap().mine(instance, blocks);
        if mined {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn mine_but_no_resume() {
    mine_but_no_resume_custom(INSTANCE_1, 1);
}

pub fn mine_but_no_resume_custom(instance: u8, blocks: u32) {
    let t_0 = OffsetDateTime::now_utc();
    loop {
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 120.0 {
            println!("forcibly breaking mining wait");
            resume_mining();
        }
        let miner = MINER.write().unwrap();
        if miner.no_mine_count <= 1 {
            miner.force_mine(instance, blocks);
            break;
        }
        drop(miner);
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn stop_mining() {
    MINER.write().unwrap().stop_mining()
}

pub fn stop_mining_when_alone() {
    let t_0 = OffsetDateTime::now_utc();
    loop {
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 120.0 {
            println!("forcibly breaking stop wait");
            stop_mining();
        }
        let mut miner = MINER.write().unwrap();
        if miner.no_mine_count == 0 {
            miner.stop_mining();
            break;
        }
        drop(miner);
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn resume_mining() {
    MINER.write().unwrap().resume_mining()
}

fn _get_connection_tuple() -> Vec<(u8, String)> {
    vec![
        (INSTANCE_3, NODE2_ADDR.to_string()), // Node 2's address
        (INSTANCE_2, NODE3_ADDR.to_string()), // Node 3's address
    ]
}

pub fn connect_reorg_nodes() {
    for (instance, node_addr) in _get_connection_tuple() {
        _bitcoin_cli_cmd(instance, vec!["addnode", &node_addr, "onetry"]);
    }

    let t_0 = OffsetDateTime::now_utc();

    loop {
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 60.0 {
            panic!("nodes failed to sync after 60 seconds");
        }

        let height_2 = get_height_custom(INSTANCE_2);
        let height_3 = get_height_custom(INSTANCE_3);

        if height_2 == height_3 {
            break;
        }

        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn disconnect_reorg_nodes() {
    for (instance, node_addr) in _get_connection_tuple() {
        _bitcoin_cli_cmd(instance, vec!["disconnectnode", &node_addr]);
    }

    for (instance, _) in _get_connection_tuple() {
        // dump peer info
        let peers = _bitcoin_cli_cmd(instance, vec!["getpeerinfo"]);
        println!(
            "disconnect_reorg_nodes:instance:{} peers: {}",
            instance, peers
        );
    }
}

pub fn get_height() -> u32 {
    get_height_custom(INSTANCE_1)
}

pub fn get_height_custom(instance: u8) -> u32 {
    _bitcoin_cli_cmd(instance, vec!["getblockcount"])
        .parse::<u32>()
        .expect("could not parse blockcount")
}

pub fn get_tx_height(txid: Txid, instance: u8) -> Option<u32> {
    let status = tx_status(txid, instance);
    if status.confirmed {
        status.block_height
    } else {
        None
    }
}

// Only Esplora
pub fn tx_status(txid: Txid, instance: u8) -> TxStatus {
    let client = esplora::Builder::new(&indexer_url(instance, Network::Regtest))
        .build_blocking()
        .unwrap();
    client.tx_status(&txid).unwrap()
}

// Only Esplora + Regtest for Lightning RGB tests
pub fn indexer_url(instance: u8, _network: Network) -> String {
    match instance {
        INSTANCE_1 => ESPLORA_1_REGTEST_URL,
        INSTANCE_2 => ESPLORA_2_REGTEST_URL,
        INSTANCE_3 => ESPLORA_3_REGTEST_URL,
        _ => ESPLORA_1_REGTEST_URL, // Default to instance 1
    }
    .to_string()
}

// Async version - only Esplora
async fn _wait_indexer_sync_async(instance: u8) {
    let t_0 = OffsetDateTime::now_utc();
    let blockcount = get_height_custom(instance);
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let url = &indexer_url(instance, Network::Regtest);
        let client = esplora::Builder::new(url).build_blocking().unwrap();
        if client.block_hash(blockcount as u32).is_ok() {
            break;
        }
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 25.0 {
            panic!("indexer not syncing with bitcoind");
        }
    }
}

// Sync version - only Esplora
fn _wait_indexer_sync(instance: u8) {
    let t_0 = OffsetDateTime::now_utc();
    let blockcount = get_height_custom(instance);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let url = &indexer_url(instance, Network::Regtest);
        let client = esplora::Builder::new(url).build_blocking().unwrap();
        if client.block_hash(blockcount as u32).is_ok() {
            break;
        }
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 25.0 {
            panic!("indexer not syncing with bitcoind");
        }
    }
}

fn _send_to_address(address: &str, sats: Option<u64>, instance: u8) -> String {
    let sats = Sats::from_sats(sats.unwrap_or(100_000_000));
    let btc = format!("{}.{:0>8}", sats.btc_floor(), sats.sats_rem());
    _bitcoin_cli_cmd(
        instance,
        vec!["-rpcwallet=miner", "sendtoaddress", address, &btc],
    )
}

pub fn fund_wallet(address: String, sats: Option<u64>, instance: u8) -> String {
    let txid = _send_to_address(&address, sats, instance);
    mine_custom(false, instance, 1);
    txid
}

pub fn get_raw_transaction(txid: &str, instance: u8) -> serde_json::Value {
    let raw_tx = _bitcoin_cli_cmd(instance, vec!["getrawtransaction", txid, "true"]);
    serde_json::from_str(&raw_tx).unwrap()
}

pub fn is_tx_confirmed(txid: &str, instance: u8) -> bool {
    let raw_tx = get_raw_transaction(txid, instance);
    if raw_tx.get("confirmations").is_none() {
        return false;
    }
    raw_tx.get("confirmations").unwrap().as_u64().unwrap() > 0
}
