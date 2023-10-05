use events::{Event, EventPublisher, EventRouter};
use telemetry::info;
use vrrb_config::NodeConfig;

use crate::{
    api::setup_rpc_api_server,
    component::NodeRuntimeComponentConfig,
    indexer_module::setup_indexer_module,
    network::{NetworkModule, NetworkModuleComponentConfig},
    node_runtime::NodeRuntime,
    result::Result,
    ui::setup_node_gui,
    RuntimeComponent, RuntimeComponentManager,
};

pub mod component;
pub mod handler_helpers;
pub mod node_runtime;
pub mod node_runtime_handler;

pub use handler_helpers::*;

pub const PULL_TXN_BATCH_SIZE: usize = 100;

pub async fn setup_runtime_components(
    original_config: &NodeConfig,
    router: &EventRouter,
    events_tx: EventPublisher,
) -> Result<(RuntimeComponentManager, NodeConfig)> {
    let mut config = original_config.clone();

    let runtime_events_rx = router.subscribe(Some("runtime-events".into()))?;
    let network_events_rx = router.subscribe(Some("network-events".into()))?;
    let jsonrpc_events_rx = router.subscribe(Some("json-rpc-api-control".into()))?;
    let indexer_events_rx = router.subscribe(None)?;

    let mut runtime_manager = RuntimeComponentManager::new();

    let node_runtime_component_handle = NodeRuntime::setup(NodeRuntimeComponentConfig {
        config: config.clone(),
        events_tx: events_tx.clone(),
        events_rx: runtime_events_rx,
    })
    .await?;

    let handle_data = node_runtime_component_handle.data();

    let node_config = handle_data.node_config.clone();

    config = node_config;

    let mempool_read_handle_factory = handle_data.mempool_read_handle_factory;
    let state_read_handle = handle_data.state_read_handle;

    runtime_manager.register_component(
        node_runtime_component_handle.label(),
        node_runtime_component_handle.handle(),
    );

    let network_component_handle = NetworkModule::setup(NetworkModuleComponentConfig {
        config: config.clone(),
        node_id: config.id.clone(),
        events_tx: events_tx.clone(),
        network_events_rx,
        vrrbdb_read_handle: state_read_handle.clone(),
        bootstrap_quorum_config: config.bootstrap_quorum_config.clone(),
        membership_config: config.quorum_config.clone(),
        validator_public_key: config.keypair.validator_public_key_owned(),
    })
    .await?;

    let resolved_network_data = network_component_handle.data();
    let network_component_handle_label = network_component_handle.label();

    runtime_manager.register_component(
        network_component_handle_label,
        network_component_handle.handle(),
    );

    config.kademlia_peer_id = Some(resolved_network_data.kademlia_peer_id);
    config.udp_gossip_address = resolved_network_data.resolved_udp_gossip_address;
    config.raptorq_gossip_address = resolved_network_data.resolved_raptorq_gossip_address;
    config.kademlia_liveness_address = resolved_network_data.resolved_kademlia_liveness_address;

    let (jsonrpc_server_handle, resolved_jsonrpc_server_addr) = setup_rpc_api_server(
        &config,
        events_tx.clone(),
        state_read_handle.clone(),
        mempool_read_handle_factory.clone(),
        jsonrpc_events_rx,
    )
    .await?;

    config.jsonrpc_server_address = resolved_jsonrpc_server_addr;

    info!("JSON-RPC server address: {}", config.jsonrpc_server_address);

    runtime_manager.register_component("API".to_string(), jsonrpc_server_handle);

    if config.enable_block_indexing {
        let handle = setup_indexer_module(&config, indexer_events_rx, mempool_read_handle_factory)?;
        // TODO: udpate this to return the proper component handle type
        // indexer_handle = Some(handle);
        // TODO: register indexer module handle
    }

    let mut node_gui_handle = None;
    if config.gui {
        node_gui_handle = setup_node_gui(&config).await?;
        info!("Node UI started");
    }

    Ok((runtime_manager, config))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ops::AddAssign;

    use block::{Block, ConvergenceBlock, QuorumId};
    use events::{AssignedQuorumMembership, Event, PeerData, DEFAULT_BUFFER};
    use hbbft::sync_key_gen::{AckOutcome, Part};
    use primitives::{generate_account_keypair, Address, NodeId, NodeType, QuorumKind};
    use theater::{ActorState, Handler};
    use validator::txn_validator;
    use vrrb_core::account::{self, Account, AccountField};
    use vrrb_core::transactions::Transaction;

    use crate::runtime::handler_helpers::*;
    use crate::test_utils::{
        create_node_runtime_network_from_nodes, create_test_network, create_txn_from_accounts,
        create_txn_from_accounts_invalid_signature, create_txn_from_accounts_invalid_timestamp,
    };
    use crate::{node_runtime::NodeRuntime, test_utils::create_node_runtime_network};

    #[tokio::test]
    #[serial_test::serial]
    async fn bootstrap_node_runtime_cannot_be_assigned_to_quorum() {
        let (events_tx, _) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(1, events_tx.clone()).await;
        let mut node = nodes.pop_front().unwrap();
        assert_eq!(node.config.node_type, NodeType::Bootstrap);

        let assigned_membership = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node.id.clone(),
            kademlia_peer_id: node.config.kademlia_peer_id.unwrap(),
            peers: vec![],
        };

        let assignment_result =
            node.handle_quorum_membership_assigment_created(assigned_membership);

        assert!(assignment_result.is_err());
        assert!(node.quorum_membership().is_none());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn validator_node_runtime_can_be_assigned_to_quorum() {
        let (events_tx, _) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(2, events_tx.clone()).await;
        nodes.pop_front().unwrap();
        let mut node = nodes.pop_front().unwrap();
        assert_eq!(node.config.node_type, NodeType::Validator);

        let assigned_membership = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node.id.clone(),
            kademlia_peer_id: node.config.kademlia_peer_id.unwrap(),
            peers: vec![],
        };

        let assignment_result =
            node.handle_quorum_membership_assigment_created(assigned_membership);

        assert!(assignment_result.is_ok());
        assert!(node.quorum_membership().is_some());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn validator_node_runtime_can_create_and_ack_partial_commitment() {
        let (events_tx, _) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(2, events_tx.clone()).await;
        nodes.pop_front().unwrap();
        let mut node = nodes.pop_front().unwrap();
        assert_eq!(node.config.node_type, NodeType::Validator);

        let assigned_membership = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node.id.clone(),
            kademlia_peer_id: node.config.kademlia_peer_id.unwrap(),
            peers: vec![],
        };

        let assignment_result =
            node.handle_quorum_membership_assigment_created(assigned_membership);

        assert!(assignment_result.is_ok());
        assert!(node.quorum_membership().is_some());

        let (part, node_id) = node.generate_partial_commitment_message().unwrap();
        assert_eq!(node_id, node.config.id);

        let (receiver_id, sender_id, ack) =
            node.handle_part_commitment_created(node_id, part).unwrap();

        assert_eq!(node.config.id, receiver_id);
        assert_eq!(node.config.id, sender_id);

        node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
            .unwrap();
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn validator_node_runtimes_can_generate_a_shared_key() {
        let (events_tx, _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut node_runtimes = create_node_runtime_network(4, events_tx.clone()).await;

        // NOTE: remove bootstrap
        node_runtimes.pop_front().unwrap();

        let mut node_1 = node_runtimes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = node_runtimes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        let ids: Vec<&primitives::QuorumId> = farmer_nodes
            .iter()
            .map(|node| node.consensus_driver.quorum_membership.as_ref().unwrap())
            .collect();
        assert_eq!(ids[0], ids[1]);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn quorum_info_is_broadcasted() {
        let (events_tx, mut _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);
        let nodes = create_test_network(4).await;
        let mut node_runtimes =
            create_node_runtime_network_from_nodes(&nodes, events_tx.clone()).await;

        // NOTE: remove bootstrap
        node_runtimes.pop_front().unwrap();

        let mut node_1 = node_runtimes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = node_runtimes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        if let Some(msg) = _rx.recv().await {
            if let messr::MessageData::Data(Event::QuorumFormed) = msg.data {
                for node in farmer_nodes {
                    let actor_state = node.handle(msg.clone()).await.unwrap();
                    assert_eq!(actor_state, ActorState::Running);
                }
            }
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn bootstrap_node_runtime_can_assign_quorum_memberships_to_available_nodes() {
        let (mut node_0, farmers, harvesters, miners) = setup_network(8).await;

        assert_eq!(farmers.len(), 4);
        assert_eq!(harvesters.len(), 2);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn bootstrap_node_runtime_can_produce_genesis_transaction() {
        let (mut node_0, farmers, harvesters, miners) = setup_network(8).await;
        node_0.produce_genesis_transactions().unwrap();

        for (_, node) in farmers.iter() {
            assert!(node.produce_genesis_transactions().is_err());
        }

        for (_, node) in harvesters.iter() {
            assert!(node.produce_genesis_transactions().is_err());
        }

        for (_, node) in miners.iter() {
            assert!(node.produce_genesis_transactions().is_err());
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn miner_node_runtime_can_mine_genesis_block() {
        let (mut node_0, farmers, harvesters, miners) = setup_network(8).await;
        let genesis_txns = node_0.produce_genesis_transactions().unwrap();

        let miner_ids = miners
            .clone()
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<NodeId>>();

        let miner_id = miner_ids.first().unwrap();

        let miner_node = miners.get(miner_id).unwrap();

        assert!(node_0.mine_genesis_block(genesis_txns.clone()).is_err());

        for harvester in harvesters.values() {
            assert!(harvester.mine_genesis_block(genesis_txns.clone()).is_err());
        }

        for farmer in farmers.values() {
            assert!(farmer.mine_genesis_block(genesis_txns.clone()).is_err());
        }

        miner_node.mine_genesis_block(genesis_txns).unwrap();
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn farmer_node_runtime_can_validate_transactions() {
        let (mut node_0, mut farmers, mut harvesters, mut miners) = setup_network(8).await;

        let (_, sender_public_key) = generate_account_keypair();
        let sender_account = Account::new(sender_public_key);
        let sender_address = node_0.create_account(sender_public_key).unwrap();

        let (_, receiver_public_key) = generate_account_keypair();
        let receiver_address = node_0.create_account(receiver_public_key).unwrap();

        let txn = create_txn_from_accounts(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        for (node_id, farmer) in farmers.iter_mut() {
            let _ = farmer.insert_txn_to_mempool(txn.clone());
            farmer
                .validate_transaction_kind(
                    txn.id(),
                    farmer.mempool_read_handle_factory().clone(),
                    farmer.state_store_read_handle_factory().clone(),
                )
                .unwrap();
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn harvester_node_runtime_can_propose_blocks() {
        let (mut node_0, farmers, mut harvesters, miners) = setup_network(8).await;

        let genesis_txns = node_0.produce_genesis_transactions().unwrap();

        let miner_ids = miners
            .clone()
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<NodeId>>();

        let miner_id = miner_ids.first().unwrap();

        let mut miner_node = miners.get(miner_id).unwrap().to_owned();
        let claim = miner_node.state_driver.dag.claim();

        let genesis_block = miner_node.mine_genesis_block(genesis_txns).unwrap();

        // TODO: impl miner elections
        // TODO: create genesis block, certify it then append it to miner's dag
        // TODO: store DAG on disk, separate from ledger

        let (_, public_key) = generate_account_keypair();
        let sender_account = Account::new(public_key);
        let sender_address = node_0.create_account(public_key).unwrap();

        let (_, public_key) = generate_account_keypair();
        let receiver_address = node_0.create_account(public_key).unwrap();

        let txn = create_txn_from_accounts(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        let mut apply_results = Vec::new();
        // let mut genesis_certs = Vec::new();

        for (_, harvester) in harvesters.iter_mut() {
            let apply_result = harvester
                .handle_block_received(Block::Genesis {
                    block: genesis_block.clone(),
                })
                .unwrap();

            // let genesis_cert = harvester
            //     .certify_genesis_block(genesis_block.clone())
            //     .unwrap();

            apply_results.push(apply_result);
            // genesis_certs.push(genesis_cert);
        }

        miner_node
            .handle_block_received(Block::Genesis {
                block: genesis_block.clone(),
            })
            .unwrap();

        for (_, harvester) in harvesters.iter_mut() {
            let proposal_block = harvester
                .mine_proposal_block(
                    genesis_block.hash.clone(),
                    Default::default(), // TODO: change to an actual map of harvester claims
                    1,
                    1,
                    claim.clone(),
                )
                .unwrap();
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn harvester_node_runtime_can_handle_genesis_block_created() {
        let (mut node_0, farmers, mut harvesters, miners) = setup_network(8).await;
        let genesis_txns = node_0.produce_genesis_transactions().unwrap();

        let miner_ids = miners
            .clone()
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<NodeId>>();

        let miner_id = miner_ids.first().unwrap();

        let miner_node = miners.get(miner_id).unwrap();

        let genesis_block = miner_node.mine_genesis_block(genesis_txns).unwrap();

        let mut apply_results = Vec::new();

        for (_, harvester) in harvesters.iter_mut() {
            let apply_result = harvester
                .handle_block_received(Block::Genesis {
                    block: genesis_block.clone(),
                })
                .unwrap();

            apply_results.push(apply_result);
        }

        for (_, harvester) in harvesters.iter_mut() {
            let txn_trie_root_hash = harvester.transactions_root_hash().unwrap();
            let state_trie_root_hash = harvester.state_root_hash().unwrap();
            for res in apply_results.iter() {
                assert_eq!(txn_trie_root_hash, res.transactions_root_hash_str());
                assert_eq!(state_trie_root_hash, res.state_root_hash_str());
            }
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "broken atm"]
    async fn harvester_node_runtime_can_handle_convergence_block_created() {
        let (mut node_0, farmers, mut harvesters, mut miners) = setup_network(8).await;
        let genesis_txns = node_0.produce_genesis_transactions().unwrap();

        let miner_ids = miners
            .clone()
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<NodeId>>();

        let miner_id = miner_ids.first().unwrap();

        let mut miner_node = miners.get_mut(miner_id).unwrap();

        let genesis_block = miner_node.mine_genesis_block(genesis_txns).unwrap();

        // TODO: impl miner elections
        // TODO: create genesis block, certify it then append it to miner's dag
        // TODO: store DAG on disk, separate from ledger

        let mut apply_results = Vec::new();
        // let mut genesis_certs = Vec::new();

        for (_, harvester) in harvesters.iter_mut() {
            let apply_result = harvester
                .handle_block_received(Block::Genesis {
                    block: genesis_block.clone(),
                })
                .unwrap();

            // let genesis_cert = harvester
            //     .certify_genesis_block(genesis_block.clone())
            //     .unwrap();

            apply_results.push(apply_result);
            // genesis_certs.push(genesis_cert);
        }

        miner_node
            .handle_block_received(Block::Genesis {
                block: genesis_block.clone(),
            })
            .unwrap();

        let convergence_block = miner_node.mine_convergence_block().unwrap();

        let mut apply_results = Vec::new();

        for (_, harvester) in harvesters.iter_mut() {
            let apply_result = harvester
                .handle_block_received(Block::Convergence {
                    block: convergence_block.clone(),
                })
                .unwrap();

            apply_results.push(apply_result);
        }

        for (_, harvester) in harvesters.iter_mut() {
            let txn_trie_root_hash = harvester.transactions_root_hash().unwrap();
            let state_trie_root_hash = harvester.state_root_hash().unwrap();
            for res in apply_results.iter() {
                assert_eq!(txn_trie_root_hash, res.transactions_root_hash_str());
                assert_eq!(state_trie_root_hash, res.state_root_hash_str());
            }
        }
        panic!();
    }

    #[tokio::test]
    #[ignore = "broken atm"]
    async fn node_runtime_can_form_quorum_with_valid_config() {
        let (mut node_0, farmers, harvesters, miners) = setup_network(8).await;

        let res = node_0.generate_partial_commitment_message();
        assert!(res.is_err(), "bootstrap nodes cannot participate in DKG");

        run_dkg_process(farmers);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn farmer_node_runtime_can_form_valid_vote_on_valid_transaction() {
        let (events_tx, _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(4, events_tx.clone()).await;

        // NOTE: remove bootstrap
        nodes.pop_front().unwrap();

        let mut node_1 = nodes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = nodes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        let ids: Vec<&primitives::QuorumId> = farmer_nodes
            .iter()
            .map(|node| node.consensus_driver.quorum_membership.as_ref().unwrap())
            .collect();

        let mut node_0 = nodes.pop_front().unwrap();

        let (_, sender_public_key) = generate_account_keypair();
        let mut sender_account = Account::new(sender_public_key);
        let update_field = AccountField::Credits(100000);
        let _ = sender_account.update_field(update_field);
        let sender_address = node_0.create_account(sender_public_key).unwrap();

        let (_, receiver_public_key) = generate_account_keypair();
        let receiver_account = Account::new(receiver_public_key);
        let receiver_address = node_0.create_account(receiver_public_key).unwrap();

        let sender_account_bytes = bincode::serialize(&sender_account.clone()).unwrap();
        let receiver_account_bytes = bincode::serialize(&receiver_account.clone()).unwrap();

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.handle_create_account_requested(
                sender_address.clone(),
                sender_account_bytes.clone(),
            );

            let _ = farmer.handle_create_account_requested(
                receiver_address.clone(),
                receiver_account_bytes.clone(),
            );
        }

        let txn = create_txn_from_accounts(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.insert_txn_to_mempool(txn.clone());
            let (transaction_kind, validity) = farmer
                .validate_transaction_kind(
                    txn.id(),
                    farmer.mempool_read_handle_factory().clone(),
                    farmer.state_store_read_handle_factory().clone(),
                )
                .unwrap();
            assert!(validity);
            farmer
                .cast_vote_on_transaction_kind(transaction_kind, validity)
                .unwrap();
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn farmer_node_runtime_can_form_invalid_vote_on_invalid_transaction_amount_greater_than_balance(
    ) {
        let (events_tx, _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(4, events_tx.clone()).await;

        // NOTE: remove bootstrap
        nodes.pop_front().unwrap();

        let mut node_1 = nodes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = nodes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        let ids: Vec<&primitives::QuorumId> = farmer_nodes
            .iter()
            .map(|node| node.consensus_driver.quorum_membership.as_ref().unwrap())
            .collect();

        let mut node_0 = nodes.pop_front().unwrap();

        let (_, sender_public_key) = generate_account_keypair();
        let mut sender_account = Account::new(sender_public_key);
        let update_field = AccountField::Credits(100);
        let _ = sender_account.update_field(update_field);
        let sender_address = node_0.create_account(sender_public_key).unwrap();

        let (_, receiver_public_key) = generate_account_keypair();
        let receiver_account = Account::new(receiver_public_key);
        let receiver_address = node_0.create_account(receiver_public_key).unwrap();

        let sender_account_bytes = bincode::serialize(&sender_account.clone()).unwrap();
        let receiver_account_bytes = bincode::serialize(&receiver_account.clone()).unwrap();

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.handle_create_account_requested(
                sender_address.clone(),
                sender_account_bytes.clone(),
            );

            let _ = farmer.handle_create_account_requested(
                receiver_address.clone(),
                receiver_account_bytes.clone(),
            );
        }

        let txn = create_txn_from_accounts(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.insert_txn_to_mempool(txn.clone());
            let (transaction_kind, validity) = farmer
                .validate_transaction_kind(
                    txn.id(),
                    farmer.mempool_read_handle_factory().clone(),
                    farmer.state_store_read_handle_factory().clone(),
                )
                .unwrap();
            assert!(!validity);
            farmer
                .cast_vote_on_transaction_kind(transaction_kind, validity)
                .unwrap();
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn farmer_node_runtime_can_form_invalid_vote_on_invalid_transaction_invalid_signature() {
        let (events_tx, _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(4, events_tx.clone()).await;

        // NOTE: remove bootstrap
        nodes.pop_front().unwrap();

        let mut node_1 = nodes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = nodes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        let ids: Vec<&primitives::QuorumId> = farmer_nodes
            .iter()
            .map(|node| node.consensus_driver.quorum_membership.as_ref().unwrap())
            .collect();

        let mut node_0 = nodes.pop_front().unwrap();

        let (_, sender_public_key) = generate_account_keypair();
        let mut sender_account = Account::new(sender_public_key);
        let update_field = AccountField::Credits(100000);
        let _ = sender_account.update_field(update_field);
        let sender_address = node_0.create_account(sender_public_key).unwrap();

        let (_, receiver_public_key) = generate_account_keypair();
        let receiver_account = Account::new(receiver_public_key);
        let receiver_address = node_0.create_account(receiver_public_key).unwrap();

        let sender_account_bytes = bincode::serialize(&sender_account.clone()).unwrap();
        let receiver_account_bytes = bincode::serialize(&receiver_account.clone()).unwrap();

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.handle_create_account_requested(
                sender_address.clone(),
                sender_account_bytes.clone(),
            );

            let _ = farmer.handle_create_account_requested(
                receiver_address.clone(),
                receiver_account_bytes.clone(),
            );
        }

        let txn = create_txn_from_accounts_invalid_signature(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.insert_txn_to_mempool(txn.clone());
            let (transaction_kind, validity) = farmer
                .validate_transaction_kind(
                    txn.id(),
                    farmer.mempool_read_handle_factory().clone(),
                    farmer.state_store_read_handle_factory().clone(),
                )
                .unwrap();
            assert!(!validity);
            farmer
                .cast_vote_on_transaction_kind(transaction_kind, validity)
                .unwrap();
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn farmer_node_runtime_can_form_invalid_vote_on_invalid_transaction_invalid_timestamp() {
        let (events_tx, _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(4, events_tx.clone()).await;

        // NOTE: remove bootstrap
        nodes.pop_front().unwrap();

        let mut node_1 = nodes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = nodes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        let ids: Vec<&primitives::QuorumId> = farmer_nodes
            .iter()
            .map(|node| node.consensus_driver.quorum_membership.as_ref().unwrap())
            .collect();

        let mut node_0 = nodes.pop_front().unwrap();

        let (_, sender_public_key) = generate_account_keypair();
        let mut sender_account = Account::new(sender_public_key);
        let update_field = AccountField::Credits(100000);
        let _ = sender_account.update_field(update_field);
        let sender_address = node_0.create_account(sender_public_key).unwrap();

        let (_, receiver_public_key) = generate_account_keypair();
        let receiver_account = Account::new(receiver_public_key);
        let receiver_address = node_0.create_account(receiver_public_key).unwrap();

        let sender_account_bytes = bincode::serialize(&sender_account.clone()).unwrap();
        let receiver_account_bytes = bincode::serialize(&receiver_account.clone()).unwrap();

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.handle_create_account_requested(
                sender_address.clone(),
                sender_account_bytes.clone(),
            );

            let _ = farmer.handle_create_account_requested(
                receiver_address.clone(),
                receiver_account_bytes.clone(),
            );
        }

        let txn = create_txn_from_accounts_invalid_timestamp(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.insert_txn_to_mempool(txn.clone());
            let (transaction_kind, validity) = farmer
                .validate_transaction_kind(
                    txn.id(),
                    farmer.mempool_read_handle_factory().clone(),
                    farmer.state_store_read_handle_factory().clone(),
                )
                .unwrap();
            assert!(!validity);
            farmer
                .cast_vote_on_transaction_kind(transaction_kind, validity)
                .unwrap();
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn farmer_node_runtime_can_form_invalid_vote_on_invalid_transaction_sender_missing() {
        let (events_tx, _rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(4, events_tx.clone()).await;

        // NOTE: remove bootstrap
        nodes.pop_front().unwrap();

        let mut node_1 = nodes.pop_front().unwrap();
        assert_eq!(node_1.config.node_type, NodeType::Validator);

        let mut node_2 = nodes.pop_front().unwrap();
        assert_eq!(node_2.config.node_type, NodeType::Validator);

        let node_1_peer_data = PeerData {
            node_id: node_1.config.id.clone(),
            node_type: node_1.config.node_type,
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_1.config.udp_gossip_address,
            raptorq_gossip_addr: node_1.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_1.config.kademlia_liveness_address,
            validator_public_key: node_1.config.keypair.validator_public_key_owned(),
        };

        let node_2_peer_data = PeerData {
            node_id: node_2.config.id.clone(),
            node_type: node_2.config.node_type,
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            udp_gossip_addr: node_2.config.udp_gossip_address,
            raptorq_gossip_addr: node_2.config.raptorq_gossip_address,
            kademlia_liveness_addr: node_2.config.kademlia_liveness_address,
            validator_public_key: node_2.config.keypair.validator_public_key_owned(),
        };

        node_1
            .handle_node_added_to_peer_list(node_2_peer_data.clone())
            .await
            .unwrap();

        node_2
            .handle_node_added_to_peer_list(node_1_peer_data.clone())
            .await
            .unwrap();

        let assigned_membership_1 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_1.id.clone(),
            kademlia_peer_id: node_1.config.kademlia_peer_id.unwrap(),
            peers: vec![node_2_peer_data],
        };

        node_1
            .handle_quorum_membership_assigment_created(assigned_membership_1)
            .unwrap();

        let assigned_membership_2 = AssignedQuorumMembership {
            quorum_kind: QuorumKind::Farmer,
            node_id: node_2.id.clone(),
            kademlia_peer_id: node_2.config.kademlia_peer_id.unwrap(),
            peers: vec![node_1_peer_data],
        };

        node_2
            .handle_quorum_membership_assigment_created(assigned_membership_2)
            .unwrap();

        let (part_1, node_id_1) = node_1.generate_partial_commitment_message().unwrap();
        let (part_2, node_id_2) = node_2.generate_partial_commitment_message().unwrap();

        let parts = vec![(node_id_1, part_1), (node_id_2, part_2)];

        let mut acks = vec![];

        for (node_id, part) in parts {
            let (receiver_id, sender_id, ack) = node_1
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));

            let (receiver_id, sender_id, ack) = node_2
                .handle_part_commitment_created(node_id.clone(), part.clone())
                .unwrap();

            acks.push((receiver_id, sender_id, ack));
        }

        let mut farmer_nodes = vec![&mut node_1, &mut node_2];

        for node in farmer_nodes.iter_mut() {
            for (receiver_id, sender_id, ack) in acks.iter().cloned() {
                node.handle_part_commitment_acknowledged(receiver_id, sender_id, ack)
                    .unwrap();
            }
        }

        for node in farmer_nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }
        for node in farmer_nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
        let ids: Vec<&primitives::QuorumId> = farmer_nodes
            .iter()
            .map(|node| node.consensus_driver.quorum_membership.as_ref().unwrap())
            .collect();

        let mut node_0 = nodes.pop_front().unwrap();

        let (_, sender_public_key) = generate_account_keypair();
        let mut sender_account = Account::new(sender_public_key);
        let update_field = AccountField::Credits(100000);
        let _ = sender_account.update_field(update_field);
        let sender_address = node_0.create_account(sender_public_key).unwrap();

        let (_, receiver_public_key) = generate_account_keypair();
        let receiver_account = Account::new(receiver_public_key);
        let receiver_address = node_0.create_account(receiver_public_key).unwrap();

        let _sender_account_bytes = bincode::serialize(&sender_account.clone()).unwrap();
        let receiver_account_bytes = bincode::serialize(&receiver_account.clone()).unwrap();

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.handle_create_account_requested(
                receiver_address.clone(),
                receiver_account_bytes.clone(),
            );
        }

        let txn = create_txn_from_accounts(
            (sender_address, Some(sender_account)),
            receiver_address,
            vec![],
        );

        for farmer in farmer_nodes.iter_mut() {
            let _ = farmer.insert_txn_to_mempool(txn.clone());
            let (transaction_kind, validity) = farmer
                .validate_transaction_kind(
                    txn.id(),
                    farmer.mempool_read_handle_factory().clone(),
                    farmer.state_store_read_handle_factory().clone(),
                )
                .unwrap();
            assert!(!validity);
            farmer
                .cast_vote_on_transaction_kind(transaction_kind, validity)
                .unwrap();
        }
    }

    async fn run_dkg_process(mut nodes: HashMap<NodeId, NodeRuntime>) {
        let mut parts = HashMap::new();

        for (node_id, node) in nodes.iter_mut() {
            let (part, node_id) = node.generate_partial_commitment_message().unwrap();
            parts.insert(node_id, part);
        }

        let parts = parts
            .into_iter()
            .map(|(node_id, part)| {
                let quorum_kind = nodes
                    .get(&node_id)
                    .unwrap()
                    .quorum_membership()
                    .unwrap()
                    .quorum_kind;

                (node_id, (part, quorum_kind))
            })
            .collect::<HashMap<NodeId, (Part, QuorumKind)>>();

        let mut acks = Vec::new();

        let mut parts_handled = 0;
        for (_, node) in nodes.iter_mut() {
            for (sender_node_id, (part, quorum_kind)) in parts.iter() {
                let ack = node
                    .handle_part_commitment_created(sender_node_id.to_owned(), part.to_owned())
                    .unwrap();

                acks.push((ack, quorum_kind));

                parts_handled += 1;
            }
        }

        for (_, node) in nodes.iter_mut() {
            for ((receiver_id, sender_id, ack), quorum_kind) in acks.iter() {
                node.handle_part_commitment_acknowledged(
                    receiver_id.to_owned(),
                    sender_id.to_owned(),
                    ack.to_owned(),
                )
                .unwrap();
            }
        }

        for (_, node) in nodes.iter_mut() {
            node.handle_all_ack_messages().unwrap();
        }

        for (_, node) in nodes.iter_mut() {
            node.generate_keysets().await.unwrap();
        }
    }

    async fn setup_network(
        n: usize,
    ) -> (
        NodeRuntime,
        HashMap<NodeId, NodeRuntime>, // farmers
        HashMap<NodeId, NodeRuntime>, // validators
        HashMap<NodeId, NodeRuntime>, // Miners
    ) {
        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let mut nodes = create_node_runtime_network(n, events_tx.clone()).await;

        let mut node_0 = nodes.pop_front().unwrap();

        let address = node_0
            .create_account(node_0.config_ref().keypair.miner_public_key_owned())
            .unwrap();

        let mut quorum_assignments = HashMap::new();

        for node in nodes.iter() {
            let peer_data = PeerData {
                node_id: node.config.id.clone(),
                node_type: node.config.node_type,
                kademlia_peer_id: node.config.kademlia_peer_id.unwrap(),
                udp_gossip_addr: node.config.udp_gossip_address,
                raptorq_gossip_addr: node.config.raptorq_gossip_address,
                kademlia_liveness_addr: node.config.kademlia_liveness_address,
                validator_public_key: node.config.keypair.validator_public_key_owned(),
            };

            let assignments = node_0
                .handle_node_added_to_peer_list(peer_data.clone())
                .await
                .unwrap();

            if let Some(assignments) = assignments {
                quorum_assignments.extend(assignments);
            }
        }

        let other_nodes_copy = nodes.clone();

        // NOTE: let nodes be aware of each other
        for node in nodes.iter_mut() {
            for other_node in other_nodes_copy.iter() {
                if node.config.id == other_node.config.id {
                    continue;
                }

                let peer_data = PeerData {
                    node_id: other_node.config.id.clone(),
                    node_type: other_node.config.node_type,
                    kademlia_peer_id: other_node.config.kademlia_peer_id.unwrap(),
                    udp_gossip_addr: other_node.config.udp_gossip_address,
                    raptorq_gossip_addr: other_node.config.raptorq_gossip_address,
                    kademlia_liveness_addr: other_node.config.kademlia_liveness_address,
                    validator_public_key: other_node.config.keypair.validator_public_key_owned(),
                };

                node.handle_node_added_to_peer_list(peer_data.clone())
                    .await
                    .unwrap();
            }
        }

        let node_0_pubkey = node_0.config_ref().keypair.miner_public_key_owned();

        // NOTE: create te bootstrap's node account as well as their accounts on everyone's state
        for current_node in nodes.iter_mut() {
            for node in other_nodes_copy.iter() {
                let node_pubkey = node.config_ref().keypair.miner_public_key_owned();
                node_0.create_account(node_pubkey).unwrap();
                current_node.create_account(node_0_pubkey).unwrap();
                current_node.create_account(node_pubkey).unwrap();
            }
        }

        let mut nodes = nodes
            .into_iter()
            .map(|node| (node.config.id.clone(), node))
            .collect::<HashMap<NodeId, NodeRuntime>>();

        let mut validator_nodes = nodes
            .clone()
            .into_iter()
            .filter(|(_, node)| node.config.node_type == NodeType::Validator)
            .collect::<HashMap<NodeId, NodeRuntime>>();

        for (node_id, node) in validator_nodes.iter_mut() {
            if let Some(assigned_membership) = quorum_assignments.get(&node.config.id) {
                node.handle_quorum_membership_assigment_created(assigned_membership.clone())
                    .unwrap();
            }
        }
        // for (_, node) in validator_nodes.iter_mut() {
        //     let quorum_kind = node.quorum_membership().unwrap().quorum_kind;
        //     if quorum_kind == QuorumKind::Farmer || quorum_kind == QuorumKind::Harvester {
        //         node.generate_keysets()
        //             .expect(&format!("failed to generate keyset for node {}", node.id));
        //     }
        // }

        let mut farmer_nodes = validator_nodes
            .clone()
            .into_iter()
            .filter(|(_, node)| node.quorum_membership().unwrap().quorum_kind == QuorumKind::Farmer)
            .collect::<HashMap<NodeId, NodeRuntime>>();

        let mut harvester_nodes = validator_nodes
            .clone()
            .into_iter()
            .filter(|(_, node)| {
                node.quorum_membership().unwrap().quorum_kind == QuorumKind::Harvester
            })
            .collect::<HashMap<NodeId, NodeRuntime>>();

        let mut miner_nodes = nodes
            .clone()
            .into_iter()
            .filter(|(_, node)| node.config.node_type == NodeType::Miner)
            .collect::<HashMap<NodeId, NodeRuntime>>();

        (node_0, farmer_nodes, harvester_nodes, miner_nodes)
    }
}
