use super::*;

static INIT: Once = Once::new();

pub static INDEXER: OnceLock<Indexer> = OnceLock::new();

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub enum Indexer {
    Electrum,
    #[default]
    Esplora,
}

impl fmt::Display for Indexer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

pub fn initialize() {
    INIT.call_once(|| {
        INDEXER.get_or_init(|| match std::env::var("INDEXER") {
            Ok(val) if val.to_lowercase() == Indexer::Esplora.to_string() => Indexer::Esplora,
            Ok(val) if val.to_lowercase() == Indexer::Electrum.to_string() => Indexer::Electrum,
            Err(VarError::NotPresent) => Indexer::Esplora,
            _ => {
                panic!("invalid indexer. possible values: `esplora` (default), `electrum`")
            }
        });
        
        if std::env::var("SKIP_INIT").is_ok() {
            println!("skipping services initialization");
            return;
        }
        
        let start_services_file = PathBuf::from("tests").join("docker").join("start_services.sh");
        println!("starting test services...");
        
        let start_output = Command::new(&start_services_file)
            .arg("start")
            .output()
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
                
                let result = _bitcoin_cli_cmd(instance, vec!["getblockchaininfo"]);
                if !result.is_empty() {
                    break;
                }
                
                attempts += 1;
                std::thread::sleep(Duration::from_secs(1));
            }
            
            // Wait for indexer sync
            _wait_indexer_sync(instance);
        }
    });
}

static MINER: Lazy<RwLock<Miner>> = Lazy::new(|| RwLock::new(Miner { no_mine_count: 0 }));

#[derive(Clone, Debug)]
pub struct Miner {
    no_mine_count: u32,
}

fn _service_base_name() -> String {
    match INDEXER.get().unwrap() {
        Indexer::Electrum => "bitcoind",
        Indexer::Esplora => "esplora",
    }
    .to_string()
}

fn _bitcoin_cli_cmd(instance: u8, args: Vec<&str>) -> String {
    let compose_file = PathBuf::from("tests").join("docker").join("docker-compose.yml");
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

    let output = Command::new("docker")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(PathBuf::from("tests").join("docker"))
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
        (INSTANCE_3, format!("172.30.2.205:18444")),  // Node 2's IP and port
        (INSTANCE_2, format!("172.30.2.206:18444")),  // Node 3's IP and port
    ]
}

pub fn connect_reorg_nodes() {
    for (instance, node_addr) in _get_connection_tuple() {
        _bitcoin_cli_cmd(instance, vec!["addnode", &node_addr, "add"]);
    }
    
    let t_0 = OffsetDateTime::now_utc();
    let mut attempt = 1;
    let max_attempts = 30;
    
    loop {
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 60.0 {
            panic!("nodes failed to sync after 60 seconds");
        }
        
        let height_2 = get_height_custom(INSTANCE_2);
        let height_3 = get_height_custom(INSTANCE_3);
        
        if height_2 == height_3 {
            // Verify connections are established
            let peers_2 = _bitcoin_cli_cmd(INSTANCE_2, vec!["getpeerinfo"]);
            let peers_3 = _bitcoin_cli_cmd(INSTANCE_3, vec!["getpeerinfo"]);
            
            if peers_2.contains("172.30.2.206") && peers_3.contains("172.30.2.205") {
                break;
            }
        }
        
        if attempt == max_attempts - 5 {
            // Retry connections if near max attempts
            for (instance, node_addr) in _get_connection_tuple() {
                _bitcoin_cli_cmd(instance, vec!["addnode", &node_addr, "add"]);
            }
        }
        
        attempt += 1;
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn disconnect_reorg_nodes() {
    for (instance, node_addr) in _get_connection_tuple() {
        _bitcoin_cli_cmd(instance, vec!["disconnectnode", &node_addr]);
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

pub fn indexer_url(instance: u8, network: Network) -> String {
    match (INDEXER.get().unwrap(), network, instance) {
        (Indexer::Electrum, Network::Mainnet, _) => ELECTRUM_MAINNET_URL,
        (Indexer::Electrum, Network::Regtest, INSTANCE_1) => ELECTRUM_1_REGTEST_URL,
        (Indexer::Electrum, Network::Regtest, INSTANCE_2) => ELECTRUM_2_REGTEST_URL,
        (Indexer::Electrum, Network::Regtest, INSTANCE_3) => ELECTRUM_3_REGTEST_URL,
        (Indexer::Esplora, Network::Mainnet, _) => ESPLORA_MAINNET_URL,
        (Indexer::Esplora, Network::Regtest, INSTANCE_1) => ESPLORA_1_REGTEST_URL,
        (Indexer::Esplora, Network::Regtest, INSTANCE_2) => ESPLORA_2_REGTEST_URL,
        (Indexer::Esplora, Network::Regtest, INSTANCE_3) => ESPLORA_3_REGTEST_URL,
        _ => unreachable!(),
    }
    .to_string()
}

fn _wait_indexer_sync(instance: u8) {
    let t_0 = OffsetDateTime::now_utc();
    let blockcount = get_height_custom(instance);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let url = &indexer_url(instance, Network::Regtest);
        match INDEXER.get().unwrap() {
            Indexer::Electrum => {
                let electrum_client = ElectrumClient::new(url).unwrap();
                if electrum_client.block_header(blockcount as usize).is_ok() {
                    break;
                }
            }
            Indexer::Esplora => {
                let esplora_client = EsploraClient::new_esplora(url).unwrap();
                if esplora_client.block_hash(blockcount).is_ok() {
                    break;
                }
            }
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
