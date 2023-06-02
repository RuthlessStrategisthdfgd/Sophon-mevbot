use node::{
    test_utils::{create_mock_bootstrap_node_config, create_mock_full_node_config_with_bootstrap},
    Node,
    NodeType,
    RuntimeModuleState,
};
use serial_test::serial;
use telemetry::TelemetrySubscriber;
use vrrb_rpc::rpc::{api::RpcApiClient, client::create_client};

#[tokio::test]
#[serial]
async fn node_can_start_as_a_bootstrap_node() {
    TelemetrySubscriber::init(std::io::stdout).unwrap();

    let node_config = create_mock_bootstrap_node_config();

    let mut vrrb_node = Node::start(&node_config).await.unwrap();

    let client = create_client(vrrb_node.jsonrpc_server_address())
        .await
        .unwrap();

    assert!(vrrb_node.is_bootstrap());
    assert_eq!(client.get_node_type().await.unwrap(), NodeType::Bootstrap);

    vrrb_node.stop();

    assert_eq!(vrrb_node.status(), RuntimeModuleState::Stopped);
}

#[tokio::test]
#[serial]
#[ignore]
async fn node_can_join_network() {
    let node_config = create_mock_bootstrap_node_config();

    let mut bootstrap_node = Node::start(&node_config).await.unwrap();

    // NOTE: use quic for peer discovery
    let bootstrap_gossip_address = bootstrap_node.udp_gossip_address();

    let node_config_1 = create_mock_full_node_config_with_bootstrap(vec![bootstrap_gossip_address]);
    let mut node_1 = Node::start(&node_config_1).await.unwrap();

    node_1.stop();
    bootstrap_node.stop();
}

#[tokio::test]
#[serial]
async fn bootstrap_node_can_add_newly_joined_peers_to_peer_list() {
    let node_config = create_mock_bootstrap_node_config();

    let mut vrrb_node = Node::start(&node_config).await.unwrap();

    let client = create_client(vrrb_node.jsonrpc_server_address())
        .await
        .unwrap();

    assert!(vrrb_node.is_bootstrap());
    assert_eq!(vrrb_node.status(), RuntimeModuleState::Stopped);
    assert_eq!(client.get_node_type().await.unwrap(), NodeType::Bootstrap);

    vrrb_node.stop();
}
