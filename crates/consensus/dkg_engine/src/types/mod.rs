pub mod config;
use std::collections::{BTreeMap, HashMap};

use hbbft::{
    crypto::{PublicKey, PublicKeySet, SecretKey, SecretKeyShare},
    sync_key_gen::{Ack, Part, SyncKeyGen},
};
use primitives::{NodeIdx, NodeType};
use rand::rngs::OsRng;
use thiserror::Error;

use crate::types::config::ThresholdConfig;

pub type NodeID = u16;
pub type SenderID = u16;

/// `DkgEngine` is a struct that holds entry point for initiating DKG
///
/// Properties:
///
/// * `node_info`: This is a struct that contains information about the node. It
///   contains the node type
/// (leader or follower) and the node index.
/// * `threshold_config`: This is the configuration for the threshold scheme. It
///   contains the number of
/// nodes in the network, the threshold, and the number of nodes that are
/// required to be online for the threshold scheme to work.
/// * `dkg_state`: This is the state of the DKG protocol. It is a struct that
///   contains the following
/// properties:
pub struct DkgEngine {
    /// To Get Info like Node Type and Node Idx
    pub node_idx: NodeIdx,

    pub node_type: NodeType,

    /// For DKG (Can be extended for heirarchical DKG)
    pub threshold_config: ThresholdConfig,

    pub secret_key: SecretKey,

    /// state information related to dkg process
    pub dkg_state: DkgState,

    /// state information related to dkg process
    pub harvester_public_key: Option<PublicKey>,
}

/// `DkgState` is a struct that contains all the state that is needed to run the
/// DKG protocol.
///
/// Properties:
/// * `part_message_store` is a hashmap that stores the `Part` messages that are
///   received from other nodes.
/// * `ack_message_store` is a hashmap that stores the `Ack` (acknowledge part
///   commitment) messages that are received from other
/// nodes.
/// * `peer_public_keys` is a BTreeMap that stores the public keys of the other
///   nodes.
/// * `public_key_set`: This is the set of public keys that are generated by the
///   DKG protocol.
/// * `secret_key_share`: This is the secret key share that is generated by the
///   DKG protocol.
/// * `sync_key_gen`: This is the key generator that will be used to generate
///   the secret key.
/// * `random_number_gen`: This is a random number generator that is used to
///   generate random numbers for the DKG protocol.
/// * `secret_key`: The secret key of the node.
pub struct DkgState {
    pub part_message_store: HashMap<NodeID, Part>,

    pub ack_message_store: HashMap<(NodeID, SenderID), Ack>,

    pub peer_public_keys: BTreeMap<NodeID, PublicKey>,

    pub public_key_set: Option<PublicKeySet>,

    pub secret_key_share: Option<SecretKeyShare>,

    pub sync_key_gen: Option<SyncKeyGen<u16>>,

    pub random_number_gen: Option<OsRng>,
}

/// List of all possible errors related to synchronous dkg generation .
#[derive(Error, Debug)]
pub enum DkgError {
    #[error("Not enough peer public messages keys to start DKG process")]
    NotEnoughPeerPublicKeys,
    #[error("Sync key Generation instance not created .")]
    SyncKeyGenInstanceNotCreated,
    #[error("Not enough part messages received")]
    NotEnoughPartMsgsReceived,
    #[error("Atleast t+1 parts needs to be completed for DKG generation to happen")]
    NotEnoughPartsCompleted,
    #[error("Not enough ack messages received")]
    NotEnoughAckMsgsReceived,
    #[error("Partial Committment not generated")]
    PartCommitmentNotGenerated,
    #[error("Partial Committment missing for node with index {0}")]
    PartMsgMissingForNode(u16),
    #[error("Partial Message already acknowledge for node with index {0}")]
    PartMsgAlreadyAcknowledge(u16),
    #[error("Invalid Part Message Error: {0}")]
    InvalidPartMessage(String),
    #[error("Invalid Ack Message Error: {0}")]
    InvalidAckMessage(String),
    #[error("Unknown error occurred while synckeygen process , Details :{0} ")]
    SyncKeyGenError(String),
    #[error("Invalid Key {0}  Value {1}")]
    ConfigInvalidValue(String, String),
    #[error("Only MasterNode should participate in DKG generation process")]
    InvalidNode,
    #[error("All participants of Quorum need to actively participate in DKG")]
    ObserverNotAllowed,
    #[error("Unknown Error: {0}")]
    Unknown(String),
}

#[derive(Debug)]
pub enum DkgResult {
    PartMessageGenerated(u16, Part),
    PartMessageAcknowledged,
    AllAcksHandled,
    KeySetsGenerated,
}

impl DkgEngine {
    pub fn new(
        node_idx: NodeIdx,
        node_type: NodeType,
        secret_key: SecretKey,
        threshold_config: ThresholdConfig,
    ) -> DkgEngine {
        DkgEngine {
            node_idx,
            node_type,
            secret_key,
            threshold_config,
            dkg_state: DkgState {
                part_message_store: HashMap::new(),
                ack_message_store: HashMap::new(),
                peer_public_keys: BTreeMap::new(),
                public_key_set: None,
                secret_key_share: None,
                sync_key_gen: None,
                random_number_gen: None,
            },
            harvester_public_key: None,
        }
    }

    pub fn add_peer_public_key(&mut self, node_id: NodeID, public_key: PublicKey) {
        self.dkg_state.peer_public_keys.insert(node_id, public_key);
    }

    pub fn get_public_key(&self) -> PublicKey {
        self.secret_key.public_key()
    }

    pub fn get_node_idx(&self) -> NodeIdx {
        self.node_idx
    }

    /// It clears the state of the DKG. it happens during change of Epoch
    pub fn clear_dkg_state(&mut self) {
        self.dkg_state.part_message_store.clear();
        self.dkg_state.ack_message_store.clear();
        self.dkg_state.sync_key_gen = None;
        self.dkg_state.random_number_gen = None;
        self.dkg_state.public_key_set = None;
        self.dkg_state.peer_public_keys.clear();
        self.dkg_state.secret_key_share = None;
    }
}
