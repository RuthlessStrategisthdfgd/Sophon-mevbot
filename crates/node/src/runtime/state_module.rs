#![allow(unused)]
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    iter::FromIterator,
    str::FromStr,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use block::{Block, BlockHash, ConvergenceBlock, ProposalBlock};
use bulldag::{graph::BullDag, vertex::Vertex};
use ethereum_types::U256;
use events::{Event, EventMessage, EventPublisher};
use lr_trie::ReadHandleFactory;
use patriecia::{db::MemoryDB, inner::InnerTrie};
use primitives::{Address, NodeId};
use storage::vrrbdb::{StateStoreReadHandle, VrrbDb, VrrbDbReadHandle};
use telemetry::info;
use theater::{Actor, ActorId, ActorLabel, ActorState, Handler, TheaterError};
use vrrb_core::{
    account::{Account, AccountDigests, AccountNonce, UpdateArgs},
    claim::Claim,
    serde_helpers::decode_from_binary_byte_slice,
    txn::{Token, TransactionDigest, Txn},
};

use crate::{result::Result, NodeError};

/// Provides a wrapper around the current rounds `ConvergenceBlock` and
/// the `ProposalBlock`s that it is made up of. Provides a convenient
/// data structure to be able to access each.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RoundBlocks {
    pub convergence: ConvergenceBlock,
    pub proposals: Vec<ProposalBlock>,
}

/// Provides variants to parse to ensure state module handles updates
/// properly, whether it be an Account receiving tokens, and
/// account sending tokens, a new claim, claim staking (TODO),
/// fees or rewards (TODO).
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum UpdateAccount {
    Sender,
    Receiver,
    Claim,
    Fee,
    Reward,
}

/// Provides a wrapper around a given account update to
/// conveniently access the data needed to produce UpdateArgs
/// which can then be consolidated into a single UpdateArgs struct
/// for each account.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StateUpdate {
    pub address: Address,
    pub token: Option<Token>,
    pub amount: u128,
    pub nonce: Option<u128>,
    pub storage: Option<String>,
    pub code: Option<String>,
    pub digest: TransactionDigest,
    pub update_account: UpdateAccount,
}

/// A wrapper to provide convenient conversion from
/// a Transaction to two StateUpdates, one for the
/// sender, one for the receiver. Can also provide some
/// verification around this struct.
// TODO: receiver update here can be used to provide
// ClaimStaking functionality, in which the `update_account`
// field for the `receiver_update` herein, can be used to
// produce a `Claim` update instead of only `Account` updates
#[derive(Debug)]
pub struct IntoUpdates {
    pub sender_update: StateUpdate,
    pub receiver_update: StateUpdate,
}

/// Provides an interface to convert a `ProposalBlock`
/// into the type that implements it
pub trait FromBlock {
    fn from_block(block: ProposalBlock) -> Self;
}

/// Provides an interface to convert a `Txn`
/// into the type that implements it
pub trait FromTxn {
    fn from_txn(txn: Txn) -> Self;
}

/// Converts a `StateUpdate` into `UpdateArgs`
impl From<StateUpdate> for UpdateArgs {
    fn from(item: StateUpdate) -> UpdateArgs {
        let mut digest = AccountDigests::default();
        match &item.update_account {
            UpdateAccount::Sender => {
                digest.insert_sent(item.digest);
                UpdateArgs {
                    address: item.address,
                    nonce: item.nonce,
                    credits: None,
                    debits: Some(item.amount),
                    storage: Some(item.storage.clone()),
                    code: Some(item.code.clone()),
                    digests: Some(digest.clone()),
                }
            },
            UpdateAccount::Receiver => {
                digest.insert_recv(item.digest);
                UpdateArgs {
                    address: item.address,
                    nonce: item.nonce,
                    credits: Some(item.amount),
                    debits: None,
                    storage: Some(item.storage.clone()),
                    code: Some(item.code.clone()),
                    digests: Some(digest.clone()),
                }
            },
            UpdateAccount::Claim => {
                // RFC: Should we separate "claim" txn from "stake" txn
                digest.insert_stake(item.digest);
                UpdateArgs {
                    address: item.address,
                    nonce: item.nonce,
                    credits: None,
                    debits: None,
                    storage: None,
                    code: None,
                    digests: Some(digest.clone()),
                }
            },
            UpdateAccount::Fee => UpdateArgs {
                address: item.address,
                nonce: item.nonce,
                credits: Some(item.amount),
                debits: None,
                storage: None,
                code: None,
                digests: None,
            },
            UpdateAccount::Reward => UpdateArgs {
                address: item.address,
                nonce: item.nonce,
                credits: Some(item.amount),
                debits: None,
                storage: None,
                code: None,
                digests: None,
            },
        }
    }
}

/// Converts a `ProposalBlock` into a `HashSet` of
/// `StateUpdate`s which can then be easily converted into
/// a `HashSet` of `UpdateArgs` to update Accounts, Claims, etc.
impl FromBlock for HashSet<StateUpdate> {
    fn from_block(block: ProposalBlock) -> Self {
        let mut set = HashSet::new();
        let mut proposer_fees = 0u128;
        block.txns.into_iter().for_each(|(digest, txn)| {
            let fee = txn.proposer_fee_share();
            proposer_fees += fee;
            let updates = IntoUpdates::from_txn(txn.clone());
            set.insert(updates.sender_update);
            set.insert(updates.receiver_update);
            let validator_fees = HashSet::<StateUpdate>::from_txn(txn);
            set.extend(validator_fees);
        });

        let fee_update = StateUpdate {
            address: block.from.address,
            token: Some(Token::default()),
            nonce: None,
            amount: proposer_fees,
            storage: None,
            code: None,
            digest: TransactionDigest::default(),
            update_account: UpdateAccount::Fee,
        };

        set.insert(fee_update);

        return set;
    }
}

/// Converts a Transaction into an `IntoUpdate`
/// which is a simple wrapper around 2 `StateUpdate`s
/// one for the sender and one for the receiver
impl FromTxn for IntoUpdates {
    fn from_txn(txn: Txn) -> IntoUpdates {
        let sender_update = StateUpdate {
            address: txn.sender_address(),
            token: Some(txn.token()),
            amount: txn.amount(),
            nonce: Some(txn.nonce()),
            storage: None,
            code: None,
            digest: txn.id(),
            update_account: UpdateAccount::Sender,
        };

        let receiver_update = StateUpdate {
            address: txn.receiver_address(),
            token: Some(txn.token()),
            amount: txn.amount(),
            nonce: None,
            storage: None,
            code: None,
            digest: txn.id(),
            update_account: UpdateAccount::Receiver,
        };

        IntoUpdates {
            sender_update,
            receiver_update,
        }
    }
}

/// Converts a Transaction into a HashSet of `StateUpdate`s
/// for fee distribution among the validators of a given tx
impl FromTxn for HashSet<StateUpdate> {
    fn from_txn(txn: Txn) -> HashSet<StateUpdate> {
        let mut set = HashSet::new();
        let fees = txn.validator_fee_share();
        let mut validator_set = txn.validators();
        validator_set.retain(|_, vote| *vote);
        let validator_share = fees / (validator_set.len() as u128);
        validator_set.iter().for_each(|(k, v)| {
            let address = Address::from_str(k);
            if let Ok(addr) = address {
                set.insert(StateUpdate {
                    address: addr,
                    token: None,
                    amount: validator_share,
                    nonce: None,
                    storage: None,
                    code: None,
                    digest: TransactionDigest::default(),
                    update_account: UpdateAccount::Fee,
                });
            }
        });

        set
    }
}

/// Provides a convenient configuration struct for buildin a
/// StateModule
pub struct StateModuleConfig {
    pub db: VrrbDb,
    pub events_tx: EventPublisher,
    pub dag: Arc<RwLock<BullDag<Block, String>>>,
}

/// The StateModule struct, which is the primary actor in
/// the module. Provides convenient access to all the data
/// necessary to transition the network's global state from
/// t to t+1.
#[derive(Debug)]
pub struct StateModule {
    db: VrrbDb,
    status: ActorState,
    label: ActorLabel,
    id: ActorId,
    events_tx: EventPublisher,
    dag: Arc<RwLock<BullDag<Block, String>>>,
}

/// StateModule manages all state persistence and updates within VrrbNodes
/// it runs as an indepdendant module such that it can be enabled and disabled
/// as necessary.
impl StateModule {
    pub fn new(config: StateModuleConfig) -> Self {
        Self {
            db: config.db,
            events_tx: config.events_tx,
            status: ActorState::Stopped,
            label: String::from("State"),
            id: uuid::Uuid::new_v4().to_string(),
            dag: config.dag.clone(),
        }
    }
}

impl StateModule {
    fn name(&self) -> String {
        String::from("State")
    }

    /// Produces a reader factory that can be used to generate read handles into
    /// the state tree.
    #[deprecated(note = "use self.read_handle instead")]
    pub fn factory(&self) -> ReadHandleFactory<InnerTrie<MemoryDB>> {
        // TODO: make this method return a custom factory
        todo!()
    }

    /// Produces the read handle for the VrrbDb instance in this
    /// struct. VrrbDbReadHandle provides a ReadHandleFactory for
    /// each of the StateStore, TransactionStore and ClaimStore.
    pub fn read_handle(&self) -> VrrbDbReadHandle {
        self.db.read_handle()
    }

    /// Inserts a Transaction into the TransactionStore and
    /// emits an event to inform other modules that a Transaction
    /// has been added to the TransactionStore.
    // This is unneccessary under the system architecture, btw.
    async fn confirm_txn(&mut self, txn: Txn) -> Result<()> {
        let txn_hash = txn.id();

        info!("Storing transaction {txn_hash} in confirmed transaction store");

        //TODO: call checked methods instead
        self.db.insert_transaction(txn)?;

        let event = Event::TxnAddedToMempool(txn_hash);

        self.events_tx.send(event.into()).await?;

        Ok(())
    }

    /// Given the hash of a `ConvergenceBlock` this method
    /// updates the StateStore, ClaimStore and TransactionStore
    /// for all new claims and transactions (excluding
    /// ClaimStaking transactions currently).
    fn update_state(&mut self, block_hash: BlockHash) -> Result<()> {
        if let Some(mut round_blocks) = self.get_proposal_blocks(block_hash) {
            consolidate_update_args(get_update_args(self.get_update_list(&mut round_blocks)))
                .into_iter()
                .for_each(|(_, args)| {
                    let _ = self.db.update_account(args);
                });

            let proposals = round_blocks.proposals.clone();

            self.update_txn_trie(&proposals);
            self.update_claim_store(&proposals);

            return Ok(());
        }

        return Err(NodeError::Other(
            "convergene block not found in DAG".to_string(),
        ));
    }

    /// Provided a reference to an array of `ProposalBlock`s
    /// making up the current round's `ConvergenceBlock`, writes all
    /// the conflict resolved transactions into the `TransactionTrie`
    fn update_txn_trie(&mut self, proposals: &[ProposalBlock]) {
        let consolidated: HashSet<Txn> = {
            let nested: Vec<HashSet<Txn>> = proposals
                .iter()
                .map(|block| block.txns.iter().map(|(_, v)| v.clone()).collect())
                .collect();

            nested.into_iter().flatten().collect()
        };

        self.db
            .extend_transactions(consolidated.into_iter().collect());
    }

    /// Provided a reference to an array of `ProposalBlock`s
    /// making up the current round's `ConvergenceBlock`, writes
    /// all the new, conflict resolved, claims into the `ClaimStore`
    fn update_claim_store(&mut self, proposals: &[ProposalBlock]) {
        let consolidated: HashSet<(U256, Claim)> = {
            let nested: Vec<HashSet<(U256, Claim)>> = {
                proposals
                    .iter()
                    .map(|block| {
                        block
                            .claims
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect()
                    })
                    .collect()
            };

            nested.into_iter().flatten().collect()
        };

        self.db.extend_claims(consolidated.into_iter().collect());
    }

    /// Provides a method to convert a `RoundBlocks` wrapper struct into
    /// a HashSet of unique `StateUpdate`s
    fn get_update_list(&self, round_blocks: &mut RoundBlocks) -> HashSet<StateUpdate> {
        let convergence = round_blocks.convergence.clone();
        let filtered_proposals: Vec<ProposalBlock> = round_blocks
            .proposals
            .iter_mut()
            .map(|block| {
                if let Some(digests) = convergence.txns.get(&block.hash) {
                    block.txns.retain(|digest, _| digests.contains(digest))
                }
                block.clone()
            })
            .collect();

        let mut updates: HashSet<StateUpdate> = HashSet::new();

        filtered_proposals.iter().for_each(|block| {
            let subset = HashSet::from_block(block.clone());
            updates.extend(subset);
        });

        updates
    }

    /// Inserts an account into the `VrrbDb` `StateStore`. This method Should
    /// only be used for *new* accounts
    fn insert_account(&mut self, key: Address, account: Account) -> Result<()> {
        self.db
            .insert_account(key, account)
            .map_err(|err| NodeError::Other(err.to_string()))
    }

    /// Returns a read handle for the StateStore to be able to read
    /// values from it.
    fn get_state_store_handle(&self) -> StateStoreReadHandle {
        self.db.state_store_factory().handle()
    }

    /// Enters into the DAG and collects and returns the current round
    /// `ConvergenceBlock` and all its source `ProposalBlock`s
    fn get_proposal_blocks(&self, index: BlockHash) -> Option<RoundBlocks> {
        let guard_result = self.dag.read();
        if let Ok(guard) = guard_result {
            let vertex_option = guard.get_vertex(index.clone());
            match &vertex_option {
                Some(vertex) => {
                    if let Block::Convergence { block } = vertex.get_data() {
                        let proposals = self.convert_sources(self.get_sources(vertex));

                        return Some(RoundBlocks {
                            convergence: block.clone(),
                            proposals,
                        });
                    }
                },
                None => {},
            }
        }

        None
    }

    /// Enters into the DAG and gets all the sources of a given vertex
    /// this is used primarily to capture all the `ProposalBlock`s
    /// that make up the current round `ConvergenceBlock`
    fn get_sources(&self, vertex: &Vertex<Block, BlockHash>) -> Vec<Vertex<Block, BlockHash>> {
        let mut source_vertices = Vec::new();
        let guard_result = self.dag.read();
        if let Ok(guard) = guard_result {
            let sources = vertex.get_sources();
            sources.iter().for_each(|index| {
                let source_option = guard.get_vertex(index.to_string());
                if let Some(source) = source_option {
                    source_vertices.push(source.clone());
                }
            });
        }

        source_vertices
    }

    /// Converts the discovered `source` `Vertex`s into a Vector of
    /// `ProposalBlock`s, filtering any other types of blocks out.
    fn convert_sources(&self, sources: Vec<Vertex<Block, BlockHash>>) -> Vec<ProposalBlock> {
        let blocks: Vec<Block> = sources.iter().map(|vtx| vtx.get_data()).collect();

        let mut proposals = Vec::new();

        blocks.iter().for_each(|block| match &block {
            Block::Proposal { block } => proposals.push(block.clone()),
            _ => {},
        });

        proposals
    }
}

/// Converts a HashSet of `StateUpdate`s into a HashSet of `UpdateArgs`s
/// structs.
fn get_update_args(updates: HashSet<StateUpdate>) -> HashSet<UpdateArgs> {
    updates.into_iter().map(|update| update.into()).collect()
}

/// Iterates through all `UpdateArgs` structs in a HashSet and consolidates
/// them into a single `UpdateArgs` struct for each address which has
/// activity in a given round.
fn consolidate_update_args(updates: HashSet<UpdateArgs>) -> HashMap<Address, UpdateArgs> {
    let mut consolidated_updates: HashMap<Address, UpdateArgs> = HashMap::new();

    for update in updates.into_iter() {
        let address = update.address.clone();

        consolidated_updates
            .entry(address)
            .and_modify(|existing_update| {
                existing_update.nonce = existing_update.nonce.max(update.nonce);
                existing_update.credits = match (existing_update.credits, update.credits) {
                    (Some(a), Some(b)) => Some(a + b),
                    (a, None) => a,
                    (_, b) => b,
                };
                existing_update.debits = match (existing_update.debits, update.debits) {
                    (Some(a), Some(b)) => Some(a + b),
                    (a, None) => a,
                    (_, b) => b,
                };
                existing_update.storage = update.storage.clone(); // TODO: Update this to use the most recent value
                existing_update.code = update.code.clone(); // TODO: Update this to use the most recent value
                if let Some(digests) = update.digests.clone() {
                    if let Some(ref mut existing_digests) = existing_update.digests {
                        existing_digests.extend_all(digests);
                    }
                }
            })
            .or_insert(update);
    }

    consolidated_updates
}


#[async_trait]
impl Handler<EventMessage> for StateModule {
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

    fn on_start(&self) {
        info!("{}-{} starting", self.label(), self.id(),);
    }

    fn on_stop(&self) {
        info!(
            "{}-{} received stop signal. Stopping",
            self.name(),
            self.label()
        );
    }

    async fn handle(&mut self, event: EventMessage) -> theater::Result<ActorState> {
        match event.into() {
            Event::Stop => {
                return Ok(ActorState::Stopped);
            },

            Event::TxnValidated(txn) => {
                self.confirm_txn(txn)
                    .await
                    .map_err(|err| TheaterError::Other(err.to_string()))?;
            },

            Event::CreateAccountRequested((address, account_bytes)) => {
                telemetry::info!(
                    "creating account {address} with new state",
                    address = address.to_string()
                );

                if let Ok(account) = decode_from_binary_byte_slice(&account_bytes) {
                    self.insert_account(address.clone(), account)
                        .map_err(|err| TheaterError::Other(err.to_string()))?;

                    telemetry::info!("account {address} created", address = address.to_string());
                }
            },
            Event::AccountUpdateRequested((address, account_bytes)) => {
                //                if let Ok(account) =
                // decode_from_binary_byte_slice(&account_bytes) {
                // self.update_account(address, account)
                // .map_err(|err| TheaterError::Other(err.to_string()))?;
                //               }
                todo!()
            },
            Event::UpdateState(block_hash) => {
                //TODO: handle error(s)
                let _ = self.update_state(block_hash);
            },
            Event::NoOp => {},
            _ => {},
        }

        Ok(ActorState::Running)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::{Arc, RwLock},
    };

    use block::Block;
    use bulldag::graph::BullDag;
    use events::{Event, DEFAULT_BUFFER};
    use serial_test::serial;
    use storage::vrrbdb::VrrbDbConfig;
    use theater::ActorImpl;
    use vrrb_core::txn::Txn;

    use super::*;

    #[tokio::test]
    #[serial]
    async fn state_runtime_module_starts_and_stops() {
        let temp_dir_path = env::temp_dir().join("state.json");

        let (events_tx, _) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let db_config = VrrbDbConfig::default();

        let dag: Arc<RwLock<BullDag<Block, String>>> = Arc::new(RwLock::new(BullDag::new()));

        let db = VrrbDb::new(db_config);

        let mut state_module = StateModule::new(StateModuleConfig {
            events_tx,
            db,
            dag: dag.clone(),
        });

        let mut state_module = ActorImpl::new(state_module);

        let (ctrl_tx, mut ctrl_rx) = tokio::sync::broadcast::channel(DEFAULT_BUFFER);

        assert_eq!(state_module.status(), ActorState::Stopped);

        let handle = tokio::spawn(async move {
            state_module.start(&mut ctrl_rx).await.unwrap();
            assert_eq!(state_module.status(), ActorState::Terminating);
        });

        ctrl_tx.send(Event::Stop.into()).unwrap();

        handle.await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn state_runtime_receives_new_txn_event() {
        let temp_dir_path = env::temp_dir().join("state.json");

        let (events_tx, _) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);
        let db_config = VrrbDbConfig::default();

        let db = VrrbDb::new(db_config);

        let dag: Arc<RwLock<BullDag<Block, String>>> = Arc::new(RwLock::new(BullDag::new()));

        let mut state_module = StateModule::new(StateModuleConfig {
            events_tx,
            db,
            dag: dag.clone(),
        });

        let mut state_module = ActorImpl::new(state_module);

        let (ctrl_tx, mut ctrl_rx) = tokio::sync::broadcast::channel(DEFAULT_BUFFER);

        assert_eq!(state_module.status(), ActorState::Stopped);

        let handle = tokio::spawn(async move {
            state_module.start(&mut ctrl_rx).await.unwrap();
        });

        ctrl_tx
            .send(Event::NewTxnCreated(Txn::null_txn()).into())
            .unwrap();

        ctrl_tx.send(Event::Stop.into()).unwrap();

        handle.await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn state_runtime_can_publish_events() {
        let temp_dir_path = env::temp_dir().join("state.json");

        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(DEFAULT_BUFFER);

        let db_config = VrrbDbConfig::default();

        let db = VrrbDb::new(db_config);

        let dag: Arc<RwLock<BullDag<Block, String>>> = Arc::new(RwLock::new(BullDag::new()));

        let mut state_module = StateModule::new(StateModuleConfig {
            events_tx,
            db,
            dag: dag.clone(),
        });

        let mut state_module = ActorImpl::new(state_module);

        let events_handle = tokio::spawn(async move {
            let res = events_rx.recv().await;
        });

        let (ctrl_tx, mut ctrl_rx) = tokio::sync::broadcast::channel(DEFAULT_BUFFER);

        assert_eq!(state_module.status(), ActorState::Stopped);

        let handle = tokio::spawn(async move {
            state_module.start(&mut ctrl_rx).await.unwrap();
        });

        // TODO: implement all state && validation ops

        ctrl_tx
            .send(Event::NewTxnCreated(Txn::null_txn()).into())
            .unwrap();

        ctrl_tx.send(Event::Stop.into()).unwrap();

        handle.await.unwrap();
        events_handle.await.unwrap();
    }
}
