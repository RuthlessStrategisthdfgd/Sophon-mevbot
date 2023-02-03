use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use config::{Config, ConfigError, File};
use derive_builder::Builder;
use primitives::{NodeId, NodeIdx, NodeType, DEFAULT_VRRB_DATA_DIR_PATH};
use serde::Deserialize;
use vrrb_core::keypair::Keypair;

use crate::bootstrap::BootstrapConfig;

#[derive(Builder, Debug, Clone, Deserialize)]
pub struct NodeConfig {
    /// UUID that identifies each node
    pub id: NodeId,

    /// Peer ID used to identify Nodes within the context of the p2p network
    pub idx: NodeIdx,

    /// Directory used to persist all VRRB node information to disk
    pub data_dir: PathBuf,

    pub db_path: PathBuf,

    /// Address the node listens for network events through RaptorQ
    pub raptorq_gossip_address: SocketAddr,

    /// Address the node listens for network events through udp2p
    pub udp_gossip_address: SocketAddr,

    /// The type of the node, used for custom impl's based on the type the
    /// capabilities may vary.
    //TODO: Change this to a generic that takes anything that implements the NodeAuth trait.
    //TODO: Create different custom structs for different kinds of nodes with different
    // authorization so that we can have custom impl blocks based on the type.
    pub node_type: NodeType,

    /// The address of the bootstrap node(s), used for peer discovery and
    /// initial state sync
    pub bootstrap_node_addresses: Vec<SocketAddr>,

    /// The address each node's HTTPs server listen to connection
    pub http_api_address: SocketAddr,

    /// An optional title meant to be displayed on API docs
    pub http_api_title: String,

    /// Version meant to be displayed on API docs
    pub http_api_version: String,

    /// Optional timeout to consider when shutting down the node's HTTP API
    /// server
    pub http_api_shutdown_timeout: Option<Duration>,

    /// Address the node listens for JSON-RPC connections
    pub jsonrpc_server_address: SocketAddr,

    // TODO: refactor env-aware options
    #[builder(default = "false")]
    pub preload_mock_state: bool,

    /// Bootstrap configuration
    pub bootstrap_config: Option<BootstrapConfig>,

    pub keypair: Keypair,
}

impl NodeConfig {
    pub fn from_file(config_path: &str) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(config_path))
            .build()?;

        Ok(s.try_deserialize().unwrap_or_default())
    }

    pub fn db_path(&self) -> &PathBuf {
        // TODO: refactor to Option and check if present and return configured db path
        // or default path within vrrb's data dir
        &self.db_path
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        let ipv4_localhost_with_random_port =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);

        Self {
            id: NodeId::default(),
            idx: NodeIdx::default(),
            data_dir: PathBuf::from(DEFAULT_VRRB_DATA_DIR_PATH),
            db_path: PathBuf::from(DEFAULT_VRRB_DATA_DIR_PATH)
                .join("node")
                .join("db"),
            raptorq_gossip_address: ipv4_localhost_with_random_port,
            udp_gossip_address: ipv4_localhost_with_random_port,
            node_type: NodeType::Full,
            bootstrap_node_addresses: vec![],
            http_api_address: ipv4_localhost_with_random_port,
            http_api_title: String::from("VRRB Node"),
            http_api_version: String::from("v.0.1.0"),
            http_api_shutdown_timeout: None,
            jsonrpc_server_address: ipv4_localhost_with_random_port,
            preload_mock_state: false,
            bootstrap_config: None,
            keypair: Keypair::random(),
        }
    }
}
