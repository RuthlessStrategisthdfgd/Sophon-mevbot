use std::net::SocketAddr;

use events::{Event, EventMessage, EventPublisher, EventRouter, Topic};
use telemetry::info;
use tokio::{
    sync::mpsc::{channel, UnboundedReceiver},
    task::JoinHandle,
};
use vrrb_config::NodeConfig;
use vrrb_core::keypair::KeyPair;

use crate::{
    result::{NodeError, Result},
    runtime::{setup_runtime_components, RuntimeHandle},
    NodeType,
    RaptorHandle,
    RuntimeModuleState,
    SchedulerHandle,
};

/// Node represents a member of the VRRB network and it is responsible for
/// carrying out the different operations permitted within the chain.
pub struct Node {
    config: NodeConfig,

    // NOTE: core node features
    router_handle: JoinHandle<()>,
    running_status: RuntimeModuleState,
    control_rx: UnboundedReceiver<Event>,
    events_tx: EventPublisher,

    // TODO: make this private
    pub keypair: KeyPair,

    // NOTE: optional node components
    state_handle: RuntimeHandle,
    mempool_handle: RuntimeHandle,
    gossip_handle: RuntimeHandle,
    miner_handle: RuntimeHandle,
    jsonrpc_server_handle: RuntimeHandle,
    quorum_election_handle: RuntimeHandle,
    dag_handle: RuntimeHandle,
    _raptor_handle: RaptorHandle,
    _scheduler_handle: SchedulerHandle,
    _grpc_server_handle: RuntimeHandle,
    node_gui_handle: RuntimeHandle,
}

pub type UnboundedControlEventReceiver = UnboundedReceiver<Event>;

impl Node {
    /// Initializes and returns a new Node instance
    pub async fn start(
        config: &NodeConfig,
        control_rx: UnboundedControlEventReceiver,
    ) -> Result<Self> {
        // Copy the original config to avoid overwriting the original
        let mut config = config.clone();

        let keypair = config.keypair.clone();

        let (events_tx, mut events_rx) = channel(events::DEFAULT_BUFFER);
        let mut router = EventRouter::new();
        router.add_topic(Topic::from("json-rpc-api-control"), Some(1));

        let runtime_components =
            setup_runtime_components(&config, &router, events_tx.clone()).await?;

        config = runtime_components.node_config;

        // TODO: report error from handle
        let router_handle = tokio::spawn(async move { router.start(&mut events_rx).await });

        info!("Node {} is ready", config.id);

        Ok(Self {
            config,
            keypair,
            events_tx,
            control_rx,
            router_handle,
            state_handle: runtime_components.state_handle,
            mempool_handle: runtime_components.mempool_handle,
            jsonrpc_server_handle: runtime_components.jsonrpc_server_handle,
            gossip_handle: runtime_components.gossip_handle,
            running_status: RuntimeModuleState::Stopped,
            miner_handle: runtime_components.miner_handle,
            quorum_election_handle: runtime_components.quorum_election_handle,
            dag_handle: runtime_components.dag_handle,
            _raptor_handle: runtime_components.raptor_handle,
            _scheduler_handle: runtime_components.scheduler_handle,
            _grpc_server_handle: runtime_components.grpc_server_handle,
            node_gui_handle: runtime_components.node_gui_handle,
        })
    }

    pub async fn wait(mut self) -> anyhow::Result<()> {
        // TODO: notify bootstrap nodes that this node is joining the network so they
        // can add it to their peer list

        info!("Launching Node {}", self.id());

        self.running_status = RuntimeModuleState::Running;

        info!("Node {} is up and running", self.id());

        // NOTE: wait for stop signal
        self.control_rx
            .recv()
            .await
            .ok_or_else(|| NodeError::Other(String::from("failed to receive control signal")))?;

        info!("Node received stop signal");

        self.events_tx.send(Event::Stop.into()).await?;

        let message = EventMessage::new(Some("json-rpc-api-control".into()), Event::Stop);
        self.events_tx.send(message).await?;

        if let Some(handle) = self.state_handle {
            handle.await??;
            info!("Shutdown complete for State module ");
        }

        if let Some(handle) = self.mempool_handle {
            handle.await??;
            info!("Shutdown complete for Mempool module ");
        }

        if let Some(handle) = self.miner_handle {
            handle.await??;
            info!("Shutdown complete for Mining module ");
        }

        if let Some(handle) = self.gossip_handle {
            handle.await??;
            info!("Shutdown complete for Broadcast module");
        }

        if let Some(handle) = self.dag_handle {
            handle.await??;
            info!("Shutdown complete for Dag module");
        }

        if let Some(handle) = self.quorum_election_handle {
            handle.await??;
            info!("Shutdown complete for Quorum election module");
        }

        // TODO: refactor this into a tokio task
        // if let Some(handle) = self.raptor_handle {
        //     handle.join();
        //     info!("shutdown complete for raptorq module");
        // }

        if let Some(handle) = self.jsonrpc_server_handle {
            handle.await??;
            info!("rpc server shut down");
        }

        if let Some(handle) = self.node_gui_handle {
            handle.await??;
            info!("node gui shut down");
        }

        self.router_handle.await?;

        info!("node shutdown complete");

        self.running_status = RuntimeModuleState::Stopped;

        Ok(())
    }

    pub async fn config(&self) -> NodeConfig {
        self.config.clone()
    }

    /// Returns a string representation of the Node id
    pub fn id(&self) -> String {
        self.config.id.clone()
    }

    /// Returns the idx of the Node
    pub fn node_idx(&self) -> u16 {
        self.config.idx
    }

    #[deprecated(note = "use node_idx instead")]
    pub fn get_node_idx(&self) -> u16 {
        self.node_idx()
    }

    /// Returns the node's type
    pub fn node_type(&self) -> NodeType {
        self.config.node_type
    }

    #[deprecated(note = "use node_type instead")]
    pub fn get_node_type(&self) -> NodeType {
        self.node_type()
    }

    pub fn is_bootsrap(&self) -> bool {
        matches!(self.node_type(), NodeType::Bootstrap)
    }

    pub fn status(&self) -> RuntimeModuleState {
        self.running_status.clone()
    }

    pub fn keypair(&self) -> KeyPair {
        self.keypair.clone()
    }

    pub fn udp_gossip_address(&self) -> SocketAddr {
        self.config.udp_gossip_address
    }

    pub fn raprtorq_gossip_address(&self) -> SocketAddr {
        self.config.raptorq_gossip_address
    }

    pub fn bootstrap_node_addresses(&self) -> Vec<SocketAddr> {
        self.config.bootstrap_node_addresses.clone()
    }

    pub fn jsonrpc_server_address(&self) -> SocketAddr {
        self.config.jsonrpc_server_address
    }
}
