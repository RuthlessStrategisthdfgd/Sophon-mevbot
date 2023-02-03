use std::{net::SocketAddr, path::PathBuf, time::Duration};

use clap::{Parser, Subcommand};
use hbbft::crypto::{serde_impl::SerdeSecret, PublicKey, SecretKey};
use node::Node;
use secp256k1::{rand, Secp256k1};
use telemetry::{error, info};
use uuid::Uuid;
use vrrb_config::NodeConfig;
use vrrb_core::{
    event_router::Event,
    keypair::{self, read_keypair_file, write_keypair_file, Keypair},
};

use crate::result::{CliError, Result};

#[derive(clap::Parser, Debug)]
pub struct RunOpts {
    /// Start node as a background process
    #[clap(short, long, action)]
    pub dettached: bool,

    ///Shows debugging config information
    #[clap(long, action, default_value = "false")]
    pub debug_config: bool,

    #[clap(short, long, value_parser)]
    pub id: primitives::NodeId,

    #[clap(long, value_parser)]
    pub idx: primitives::NodeIdx,

    /// Defines the type of node created by this program
    #[clap(short = 't', long, value_parser, default_value = "full")]
    pub node_type: String,

    #[clap(long, value_parser)]
    pub data_dir: PathBuf,

    #[clap(long, value_parser)]
    pub db_path: PathBuf,

    #[clap(long, value_parser)]
    pub udp_gossip_address: SocketAddr,

    #[clap(long, value_parser)]
    pub raptorq_gossip_address: SocketAddr,

    #[clap(long, value_parser)]
    pub http_api_address: SocketAddr,

    #[clap(long)]
    pub bootstrap: bool,

    #[clap(long, value_parser)]
    pub bootstrap_node_addresses: Vec<SocketAddr>,

    /// Title of the API shown on swagger docs
    #[clap(long, value_parser, default_value = "Node RPC API")]
    pub http_api_title: String,

    /// API version shown in swagger docs
    #[clap(long, value_parser)]
    pub http_api_version: String,
}

#[derive(Debug, Subcommand)]
pub enum NodeCmd {
    /// Run a node with the provided configuration
    Run(RunOpts),

    /// Prints currrent node configuration
    Info,

    /// Stops any currently running node if any
    Stop,
}

#[derive(Parser, Debug)]
pub struct NodeOpts {
    /// Sets a custom config file
    #[clap(short, long, value_parser, value_name = "FILE")]
    pub config: Option<PathBuf>,

    #[clap(subcommand)]
    pub subcommand: NodeCmd,
}

pub async fn exec(args: NodeOpts) -> Result<()> {
    let sub_cmd = args.subcommand;

    match sub_cmd {
        NodeCmd::Run(opts) => run(opts, args.config).await,
        NodeCmd::Info => {
            if let Some(config_file_path) = args.config {
                let node_config = read_node_config_from_file(config_file_path)?;

                dbg!(node_config);
            }

            Ok(())
        },
        _ => Err(CliError::InvalidCommand(format!("{:?}", sub_cmd))),
    }
}

pub fn read_node_config_from_file(config_file_path: PathBuf) -> Result<NodeConfig> {
    let path_str = config_file_path.to_str().unwrap_or_default();

    let node_config = NodeConfig::from_file(path_str)
        .map_err(|err| CliError::Other(format!("Failed to read config file: {}", err)))?;

    Ok(node_config)
}

/// Configures and runs a VRRB Node
pub async fn run(args: RunOpts, config_file_path: Option<PathBuf>) -> Result<()> {
    let mut base_node_config = NodeConfig::default();

    if let Some(config_file_path) = config_file_path {
        base_node_config = read_node_config_from_file(config_file_path)?;
    }

    // TODO: get these from proper config
    let id = Uuid::new_v4().to_simple().to_string();
    // let idx = args.idx;
    // let node_type = args.node_type.parse()?;
    let data_dir = storage::get_node_data_dir()?;
    // let db_path = args.db_path;
    // let bootstrap = args.bootstrap;
    // let bootstrap_node_addresses = args.bootstrap_node_addresses;

    // let udp_gossip_address = args.udp_gossip_address;
    // let raptorq_gossip_address = args.raptorq_gossip_address;

    // let http_api_address = args.http_api_address;

    // let http_api_title = args.http_api_title;
    // let http_api_version = args.http_api_version;
    let http_api_shutdown_timeout = Some(Duration::from_secs(5));

    // TODO: refactor key reads
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();
    let (secret_key, pubkey) = secp.generate_keypair(&mut rng);

    let keypair_file_path = PathBuf::from(&data_dir).join("keypair");

    let keypair = match read_keypair_file(&keypair_file_path) {
        Ok(keypair) => keypair,
        Err(err) => {
            error!("Failed to read keypair file: {}", err);
            info!("Generating new keypair");
            let keypair = Keypair::random();

            write_keypair_file(&keypair, &keypair_file_path)
                .map_err(|err| CliError::Other(format!("Failed to write keypair file: {}", err)))?;

            keypair
        },
    };

    let node_config = NodeConfig {
        id,
        data_dir,
        keypair,
        http_api_shutdown_timeout,
        ..base_node_config
    };

    if args.debug_config {
        dbg!(&node_config);
    }

    if args.dettached {
        run_dettached(node_config).await
    } else {
        run_blocking(node_config).await
    }
}

#[telemetry::instrument]
async fn run_blocking(node_config: NodeConfig) -> Result<()> {
    let (ctrl_tx, mut ctrl_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    let vrrb_node = Node::start(&node_config, ctrl_rx)
        .await
        .map_err(|err| CliError::Other(String::from("failed to listen for ctrl+c")))?;

    let node_type = vrrb_node.node_type();

    info!("running {node_type:?} node in blocking mode");

    let node_handle = tokio::spawn(async move {
        // NOTE: starts the main node service
        vrrb_node.wait().await
    });

    tokio::signal::ctrl_c()
        .await
        .map_err(|_| CliError::Other(String::from("failed to listen for ctrl+c")))?;

    ctrl_tx
        .send(Event::Stop)
        .map_err(|_| CliError::Other(String::from("failed to send stop event to node")))?;

    node_handle
        .await
        .map_err(|_| CliError::Other(String::from("failed to join node task handle")))?;

    info!("node stopped");

    Ok(())
}

#[telemetry::instrument]
async fn run_dettached(node_config: NodeConfig) -> Result<()> {
    info!("running node in dettached mode");
    // start child process, run node within it
    Ok(())
}
