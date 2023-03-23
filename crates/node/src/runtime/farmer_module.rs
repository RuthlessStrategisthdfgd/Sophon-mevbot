use std::{
    borrow::{Borrow, BorrowMut},
    thread,
};

use async_trait::async_trait;
use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;
use hbbft::{crypto::{Signature, SignatureShare}, threshold_sign::ThresholdSign};
use indexmap::IndexMap;
use kademlia_dht::{Key, Node, NodeData};
use lr_trie::ReadHandleFactory;
use mempool::{mempool::{LeftRightMempool, TxnStatus}, Mempool};
use patriecia::{db::MemoryDB, inner::InnerTrie};
use primitives::{
    FarmerQuorumThreshold,
    GroupPublicKey,
    HarvesterQuorumThreshold,
    NodeIdx,
    PeerId,
    QuorumType,
    RawSignature,
    TxHashString,
};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use signer::signer::{SignatureProvider, Signer};
use telemetry::info;
use theater::{Actor, ActorId, ActorLabel, ActorState, Handler, Message, TheaterError};
use tokio::sync::{broadcast::error::TryRecvError, mpsc::UnboundedSender};
use tracing::error;
use vrrb_core::{
    bloom::Bloom,
    event_router::{DirectedEvent, Event, QuorumCertifiedTxn, Topic, Vote, VoteReceipt},
    txn::{TransactionDigest, Txn},
};

use crate::{
    result::Result,
    scheduler::{Job, JobResult},
    NodeError, validator_module::ValidatorModule, farmer_harvester_module::QuorumMember,
};


pub type QuorumId = String;
pub type QuorumPubkey = String;

#[allow(unused)]
pub struct Farmer {
    pub tx_mempool_reader: ReadHandleFactory<Mempool>,
    pub group_public_key: GroupPublicKey,
    pub members: Vec<PeerId>,
    pub sig_provider: Option<SignatureProvider>,
    pub farmer_id: PeerId,
    pub farmer_node_idx: NodeIdx,
    validator_module: ValidatorModule,
    status: ActorState,
    label: ActorLabel,
    id: ActorId,
    broadcast_events_tx: tokio::sync::mpsc::UnboundedSender<DirectedEvent>,
    clear_filter_rx: tokio::sync::mpsc::UnboundedReceiver<DirectedEvent>,
    farmer_quorum_threshold: FarmerQuorumThreshold,
    sync_jobs_sender: Sender<Job>,
    async_jobs_sender: Sender<Job>,
    sync_jobs_status_receiver: Receiver<JobResult>,
    async_jobs_status_receiver: Receiver<JobResult>,
}


impl QuorumMember for Farmer {}


impl Farmer {
    pub fn new(
        tx_mempool_reader: ReadHandleFactory<Mempool>,
        sig_provider: Option<SignatureProvider>,
        group_public_key: GroupPublicKey,
        members: Vec<PeerId>,
        farmer_id: PeerId,
        farmer_node_idx: NodeIdx,
        broadcast_events_tx: tokio::sync::mpsc::UnboundedSender<DirectedEvent>,
        clear_filter_rx: tokio::sync::mpsc::UnboundedReceiver<DirectedEvent>,
        farmer_quorum_threshold: FarmerQuorumThreshold,
        validator_module: ValidatorModule,
        sync_jobs_sender: Sender<Job>,
        async_jobs_sender: Sender<Job>,
        sync_jobs_status_receiver: Receiver<JobResult>,
        async_jobs_status_receiver: Receiver<JobResult>,
    ) -> Self {

        Self {
            tx_mempool_reader,
            group_public_key,
            members,
            sig_provider,
            farmer_id,
            farmer_node_idx,
            validator_module,
            status: ActorState::Stopped,
            label: String::from("FarmerHarvester"),
            id: uuid::Uuid::new_v4().to_string(),
            broadcast_events_tx: broadcast_events_tx.clone(),
            clear_filter_rx,
            farmer_quorum_threshold,
            sync_jobs_sender,
            async_jobs_sender,
            sync_jobs_status_receiver: sync_jobs_status_receiver.clone(),
            async_jobs_status_receiver: async_jobs_status_receiver.clone(),
        }
    }

    pub fn name(&self) -> String {
        self.label.clone()
    }
}


#[async_trait]
impl Handler<Event> for Farmer {
    fn id(&self) -> ActorId {
        self.id.clone()
    }

    fn label(&self) -> ActorLabel {
        self.name()
    }

    fn status(&self) -> ActorState {
        self.status.clone()
    }

    fn set_status(&mut self, actor_status: ActorState) {
        self.status = actor_status;
    }

    fn on_stop(&self) {
        info!(
            "{}-{} received stop signal. Stopping",
            self.name(),
            self.label()
        );
    }

    async fn handle(&mut self, event: Event) -> theater::Result<ActorState> {
        match event {
            Event::Stop => {
                return Ok(ActorState::Stopped);
            },
            Event::Vote(vote, quorum, farmer_quorum_threshold) => {
                // Send Vote to the Harvester Quorum:
                self.vote_valid()

            },
            Event::PullQuorumCertifiedTxns(num_of_txns) => {
            },
            Event::NoOp => {},
            _ => {},
        }

        Ok(ActorState::Running)
    }
}

impl Harvester {
    pub fn new(
        certified_txns_filter: Bloom,
        farmer_quorum_pubkeys: DashMap<QuorumId, QuorumPubkey>,
        farmer_quorum_members: DashMap<QuorumId, Vec<PeerId>>,
        sig_provider: Option<SignatureProvider>,
        group_public_key: GroupPublicKey,
        harvester_id: PeerId,
        harvester_node_idx: NodeIdx,
        broadcast_events_tx: tokio::sync::mpsc::UnboundedSender<DirectedEvent>,
        clear_filter_rx: tokio::sync::mpsc::UnboundedReceiver<DirectedEvent>,
        farmer_quorum_threshold: FarmerQuorumThreshold,
        harvester_quorum_threshold: HarvesterQuorumThreshold,
        sync_jobs_sender: Sender<Job>,
        async_jobs_sender: Sender<Job>,
        sync_jobs_status_receiver: Receiver<JobResult>,
        async_jobs_status_receiver: Receiver<JobResult>,
    ) -> Self {

        // Need to discuss how the new harvester 
        // takes over existing transaction votes.
        // When a "new" election occurs,
        // we need to have a wind down where txs that
        // are pending are completed before the new
        // quorum takes over.
        Self {
            certified_txns_filter,
            votes_pool: DashMap::new(),
            farmer_quorum_pubkeys,
            farmer_quorum_members,
            group_public_key,
            sig_provider,
            harvester_id,
            harvester_node_idx,
            status: ActorState::Stopped,
            label: String::from("FarmerHarvester"),
            id: uuid::Uuid::new_v4().to_string(),
            broadcast_events_tx,
            clear_filter_rx,
            farmer_quorum_threshold,
            harvester_quorum_threshold,
            sync_jobs_sender,
            async_jobs_sender,
            sync_jobs_status_receiver,
            async_jobs_status_receiver,
        }
    }

    pub fn name(&self) -> String {
        self.label.clone()
    }
}

