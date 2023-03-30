use std::{collections::HashMap, net::SocketAddr};

use block::convergence_block::ConvergenceBlock;
use primitives::{
    Address,
    ByteVec,
    FarmerQuorumThreshold,
    HarvesterQuorumThreshold,
    NodeIdx,
    NodeType,
    PeerId,
    NodeId,
    QuorumPublicKey,
    QuorumType,
    RawSignature,
    TransactionDigest,
    TxHashString,
};
use serde::{Deserialize, Serialize};
use telemetry::{error, info};
use tokio::sync::{
    broadcast::{self, Sender},
    mpsc::{UnboundedReceiver, UnboundedSender},
};
use vrrb_core::{
    account::Account,
    txn::{TransactionDigest, Txn},
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde_json error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Subscriber = UnboundedSender<Event>;
pub type Publisher = UnboundedSender<(Topic, Event)>;
pub type AccountBytes = Vec<u8>;
pub type BlockBytes = Vec<u8>;
pub type HeaderBytes = Vec<u8>;
pub type ConflictBytes = Vec<u8>;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerData {
    pub address: SocketAddr,
    pub node_type: NodeType,
    pub peer_id: PeerId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct SyncPeerData {
    pub address: SocketAddr,
    pub raptor_udp_port: u16,
    pub quic_port: u16,
    pub node_type: NodeType,
}

// NOTE: naming convention for events goes as follows:
// <Subject><Verb, in past tense>, e.g. ObjectCreated
// TODO: Replace Vec<u8>'s with proper data structs in enum wariants
// once definitions of those are moved into primitives.

#[derive(Debug, Deserialize, Serialize, Hash, Clone, PartialEq, Eq)]
pub struct Vote {
    /// The identity of the voter.
    pub farmer_id: Vec<u8>,
    pub farmer_node_id: NodeIdx,
    /// Partial Signature
    pub signature: RawSignature,
    pub txn: Txn,
    pub execution_result: Option<String>,
    pub quorum_public_key: Vec<u8>,
    pub quorum_threshold: usize,
    // May want to serialize this as a vector of bytes
    pub execution_result: Option<String>,
}

pub type SerializedConvergenceBlock = ByteVec;

#[derive(Debug, Deserialize, Serialize, Hash, Clone, PartialEq, Eq)]
pub struct BlockVote {
    pub harvester_id: Vec<u8>,
    pub harvester_node_id: NodeIdx,
    pub signature: RawSignature,
    pub convergence_block: SerializedConvergenceBlock,
    pub quorum_public_key: Vec<u8>,
    pub quorum_threshold: usize,
}

#[derive(Debug, Deserialize, Serialize, Hash, Clone, PartialEq, Eq)]
pub struct VoteReceipt {
    /// The identity of the voter.
    pub farmer_id: Vec<u8>,
    pub farmer_node_id: NodeIdx,
    /// Partial Signature
    pub signature: RawSignature,
}

#[derive(Default, Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuorumCertifiedTxn {
    sender_farmer_id: Vec<u8>,
    /// All valid vote receipts
    votes: Vec<VoteReceipt>,
    txn: Txn,
    /// Threshold Signature
    signature: RawSignature,
}

#[derive(Default, Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Event {
    #[default]
    NoOp,
    Stop,

    /// New txn came from network, requires validation
    NewTxnCreated(Txn),
    /// Single txn validated
    TxnValidated(Txn),
    /// Batch of validated txns
    TxnBatchValidated(Vec<TransactionDigest>),
    TxnAddedToMempool(TransactionDigest),
    MempoolSizeThesholdReached {
        cutoff_transaction: TransactionDigest,
    },
    BlockReceived,
    BlockConfirmed(Vec<u8>),
    ClaimCreated(Vec<u8>),
    ClaimProcessed(Vec<u8>),
    UpdateLastBlock(Vec<u8>),
    ClaimAbandoned(String, Vec<u8>),
    SlashClaims(Vec<String>),
    CheckAbandoned,
    SyncPeers(Vec<SyncPeerData>),
    EmptyPeerSync,
    PeerSyncFailed(Vec<SocketAddr>),
    PeerRequestedStateSync(PeerData),

    //Event to tell Farmer node to sign the Transaction
    //the validator module has validated this transaction
    ValidTxn(TransactionDigest),
    /// A peer joined the network, should be added to the node's peer list
    PeerJoined(PeerData),

    /// Peer abandoned the network. Should be removed from the node's peer list
    PeerLeft(PeerData),

    /// A Event to start the DKG process.
    DkgInitiate,

    /// A command to  ack Part message of  sender .
    AckPartCommitment(u16),

    /// Event to broadcast Part Message
    PartMessage(u16, Vec<u8>),

    /// Event to broadcast Part Message
    SendPartMessage(u16, Vec<u8>),

    /// A command to  send ack of Part message of sender by current Node.
    SendAck(u16, u16, Vec<u8>),

    /// A command to handle all the acks received by the node.
    HandleAllAcks,

    /// Used to generate the public key set& Distrbuted Group Public Key for the
    /// node.
    GenerateKeySet,
    HarvesterPublicKey(Vec<u8>),
    Farm,
    Vote(Vote, QuorumType, FarmerQuorumThreshold),
    PullQuorumCertifiedTxns(usize),
    QuorumCertifiedTxns(QuorumCertifiedTxn),

    ConfirmedTxns(Vec<(String, QuorumPublicKey)>),

    CreateAccountRequested((Address, AccountBytes)),
    AccountCreated(Address),

    AccountUpdateRequested((Address, AccountBytes)),
    UpdatedAccount(AccountBytes),
    MinerElection(HeaderBytes),
    QuorumElection(HeaderBytes),
    ConflictResolution(ConflictBytes, HeaderBytes),

    // SendTxn(u32, String, u128), // address number, receiver address, amount
    // ProcessTxnValidator(Vec<u8>),
    // PendingBlock(Vec<u8>, String),
    // InvalidBlock(Vec<u8>),
    // ProcessClaim(Vec<u8>),
    // CheckStateUpdateStatus((u128, Vec<u8>, u128)),
    // StateUpdateCompleted(Vec<u8>),
    // StoreStateDbChunk(Vec<u8>, Vec<u8>, u32, u32),
    // SendState(String, u128),
    // SendMessage(SocketAddr, Message),
    // GetBalance(u32),
    // SendGenesis(String),
    // SendStateComponents(String, Vec<u8>, String),
    // GetStateComponents(String, Vec<u8>, String),
    // RequestedComponents(String, Vec<u8>, String, String),
    // StoreStateComponents(Vec<u8>, ComponentTypes),
    // StoreChild(Vec<u8>),
    // StoreParent(Vec<u8>),
    // StoreGenesis(Vec<u8>),
    // StoreLedger(Vec<u8>),
    // StoreNetworkState(Vec<u8>),
    // StateUpdateComponents(Vec<u8>, ComponentTypes),
    // UpdateAppMiner(Vec<u8>),
    // UpdateAppBlockchain(Vec<u8>),
    // UpdateAppMessageCache(Vec<u8>),
    // UpdateAppWallet(Vec<u8>),
    // Publish(Vec<u8>),
    // Gossip(Vec<u8>),
    // AddNewPeer(String, String),
    // AddKnownPeers(Vec<u8>),
    // AddExplicitPeer(String, String),
    // ProcessPacket((Packet, SocketAddr)),
    // Bootstrap(String, String),
    // SendPing(String),
    // ReturnPong(Vec<u8>, String),
    // InitHandshake(String),
    // ReciprocateHandshake(String, String, String),
    // CompleteHandshake(String, String, String),
    // ProcessAck(String, u32, String),
    // CleanInbox(String),
    // StartMiner,
    // GetHeight,
    // MineBlock,
    // MineGenesis,
    // StopMine,
    // GetState,
    // ProcessBacklog,
    // SendAddress,
    // NonceUp,
    // InitDKG,
    // SendPartMessage(Vec<u8>),
    // SendAckMessage(Vec<u8>),
    // PublicKeySetSync,
}

impl From<&theater::Message> for Event {
    fn from(msg: &theater::Message) -> Self {
        serde_json::from_slice(&msg.data).unwrap_or_default()
    }
}

impl From<theater::Message> for Event {
    fn from(msg: theater::Message) -> Self {
        serde_json::from_slice(&msg.data).unwrap_or_default()
    }
}

impl From<Vec<u8>> for Event {
    fn from(data: Vec<u8>) -> Self {
        serde_json::from_slice(&data).unwrap_or_default()
    }
}

impl From<Event> for Vec<u8> {
    fn from(evt: Event) -> Self {
        serde_json::to_vec(&evt).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
/// Contains all the potential topics.
pub enum Topic {
    Control,
    Internal,
    External,
    State,
    Network,
    Storage,
    Consensus,
    Throttle,
}

/// EventRouter is an internal message bus that coordinates interaction
/// between runtime modules.
pub struct EventRouter {
    /// Map of async transmitters to various runtime modules
    topics: HashMap<Topic, Sender<Event>>,
}

pub type DirectedEvent = (Topic, Event);

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRouter {
    pub fn new() -> Self {
        Self {
            topics: HashMap::new(),
        }
    }

    pub fn add_topic(&mut self, topic: Topic, size: Option<usize>) {
        let buffer = size.unwrap_or(1);
        let (tx, _) = broadcast::channel(buffer);

        self.topics.insert(topic, tx);
    }

    pub fn subscribe(
        &self,
        topic: &Topic,
    ) -> std::result::Result<broadcast::Receiver<Event>, Error> {
        if let Some(sender) = self.topics.get(topic) {
            Ok(sender.subscribe())
        } else {
            Err(Error::Other(format!("unable to subscribe to {topic:?}")))
        }
    }

    /// Starts the event router, distributing all incomming events to all
    /// subscribers
    pub async fn start(&mut self, event_rx: &mut UnboundedReceiver<DirectedEvent>) {
        while let Some((topic, event)) = event_rx.recv().await {
            if event == Event::Stop {
                info!("event router received stop signal");
                self.fan_out_event(Event::Stop, &topic);

                return;
            }

            self.fan_out_event(event, &topic);
        }
    }

    fn fan_out_event(&mut self, event: Event, topic: &Topic) {
        if let Some(topic_sender) = self.topics.get_mut(topic) {
            if let Err(err) = topic_sender.send(event.clone()) {
                error!("failed to send event {event:?} to topic {topic:?}: {err:?}");
            }
        }
    }
}

impl QuorumCertifiedTxn {
    pub fn new(
        sender_farmer_id: Vec<u8>,
        votes: Vec<VoteReceipt>,
        txn: Txn,
        signature: RawSignature,
    ) -> QuorumCertifiedTxn {
        QuorumCertifiedTxn {
            sender_farmer_id,
            votes,
            txn,
            signature,
        }
    }
}
#[cfg(test)]
mod tests {

    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    #[tokio::test]
    async fn should_susbcribe_to_topics() {
        let mut router = EventRouter::new();

        router.add_topic(Topic::Control, None);

        router.subscribe(&Topic::Control).unwrap();
    }

    #[tokio::test]
    async fn should_stop_when_issued_stop_event() {
        let (event_tx, mut event_rx) = unbounded_channel::<DirectedEvent>();
        let mut router = EventRouter::new();

        router.add_topic(Topic::Control, Some(10));

        let mut subscriber_rx = router.subscribe(&Topic::Control).unwrap();

        let handle = tokio::spawn(async move {
            router.start(&mut event_rx).await;
        });

        event_tx.send((Topic::Control, Event::Stop)).unwrap();

        handle.await.unwrap();

        assert_eq!(subscriber_rx.try_recv().unwrap(), Event::Stop);
    }
}

// NOTE: kept for reference
//
/// Command represents the vocabulary of available RPC-style interactions with
/// VRRB node internal components. Commands are meant to be issued by a command
/// router that controls node runtime modules.
//TODO: Review all the commands and determine which ones are needed, which can be changed
#[deprecated(note = "use Event instead")]
#[derive(Debug, Clone)]
pub enum Command {
    //TODO: Replace standard types with custom types for better readability
    // and to help engineers understand what the hell these items are.
    SendTxn(u32, String, u128), // address number, receiver address, amount
    ProcessTxn(Vec<u8>),
    ProcessTxnValidator(Vec<u8>),
    ConfirmedBlock(Vec<u8>),
    PendingBlock(Vec<u8>, String),
    InvalidBlock(Vec<u8>),
    ProcessClaim(Vec<u8>),
    CheckStateUpdateStatus((u128, Vec<u8>, u128)),
    StateUpdateCompleted(Vec<u8>),
    StoreStateDbChunk(Vec<u8>, Vec<u8>, u32, u32),
    SendState(String, u128),
    // SendMessage(SocketAddr, Message),
    GetBalance(u32),
    SendGenesis(String),
    SendStateComponents(String, Vec<u8>, String),
    GetStateComponents(String, Vec<u8>, String),
    RequestedComponents(String, Vec<u8>, String, String),
    // StoreStateComponents(Vec<u8>, ComponentTypes),
    StoreChild(Vec<u8>),
    StoreParent(Vec<u8>),
    StoreGenesis(Vec<u8>),
    StoreLedger(Vec<u8>),
    StoreNetworkState(Vec<u8>),
    // StateUpdateComponents(Vec<u8>, ComponentTypes),
    UpdateLastBlock(Vec<u8>),
    ClaimAbandoned(String, Vec<u8>),
    SlashClaims(Vec<String>),
    UpdateAppMiner(Vec<u8>),
    UpdateAppBlockchain(Vec<u8>),
    UpdateAppMessageCache(Vec<u8>),
    UpdateAppWallet(Vec<u8>),
    Publish(Vec<u8>),
    Gossip(Vec<u8>),
    AddNewPeer(String, String),
    AddKnownPeers(Vec<u8>),
    AddExplicitPeer(String, String),
    // ProcessPacket((Packet, SocketAddr)),
    Bootstrap(String, String),
    SendPing(String),
    ReturnPong(Vec<u8>, String),
    InitHandshake(String),
    ReciprocateHandshake(String, String, String),
    CompleteHandshake(String, String, String),
    ProcessAck(String, u32, String),
    CleanInbox(String),
    CheckAbandoned,
    StartMiner,
    GetHeight,
    MineBlock,
    MineGenesis,
    StopMine,
    GetState,
    ProcessBacklog,
    SendAddress,
    NonceUp,
    InitDKG,
    SendPartMessage(Vec<u8>),
    SendAckMessage(Vec<u8>),
    PublicKeySetSync,
    Stop,
    NoOp,
}
