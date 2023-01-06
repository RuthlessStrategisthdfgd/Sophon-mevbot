use std::{
    collections::HashSet,
    hash::Hash,
    result::Result as StdResult,
    time::{SystemTime, UNIX_EPOCH},
};

use fxhash::FxBuildHasher;
use indexmap::IndexMap;
use left_right::{Absorb, ReadHandle, ReadHandleFactory, WriteHandle};
use primitives::TxHashString;
use serde::{Deserialize, Serialize};
use vrrb_core::txn::{TxTimestamp, Txn};

use super::error::MempoolError;

pub type Result<T> = StdResult<T, MempoolError>;

//TODO: simplify mempool

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Default)]
pub struct TxnRecord {
    pub txn_id: TxHashString,
    pub txn: Txn,
    pub status: TxnStatus,
    pub timestamp: TxTimestamp,
    pub added_timestamp: TxTimestamp,
    pub validated_timestamp: TxTimestamp,
    pub rejected_timestamp: TxTimestamp,
    pub deleted_timestamp: TxTimestamp,
}

impl TxnRecord {
    pub fn new(txn: Txn) -> TxnRecord {
        let timestamp = chrono::offset::Utc::now().timestamp();

        TxnRecord {
            txn_id: txn.digest(),
            txn,
            timestamp: txn.timestamp,
            added_timestamp: timestamp,
            ..Default::default()
        }
    }

    pub fn new_by_id(txn_id: &str) -> TxnRecord {
        TxnRecord {
            txn_id: txn_id.to_owned(),
            ..Default::default()
        }
    }
}

pub type PoolType = IndexMap<TxHashString, TxnRecord, FxBuildHasher>;

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxnStatus {
    #[default]
    Pending,
    Validated,
    Rejected,
}

/// Mempool stores unprocessed transactions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mempool {
    pool: PoolType,
}

impl Default for Mempool {
    fn default() -> Self {
        // TODO - to be moved to a common configuration file
        let initial_mempool_capacity = 100;

        Mempool {
            pending: MempoolType::with_capacity_and_hasher(
                initial_mempool_capacity,
                <_>::default(),
            ),
            validated: MempoolType::with_capacity_and_hasher(
                initial_mempool_capacity,
                <_>::default(),
            ),
            rejected: MempoolType::with_capacity_and_hasher(
                initial_mempool_capacity,
                <_>::default(),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MempoolOp {
    Add(TxnRecord),
    Remove(TxHashString),
}

impl Absorb<MempoolOp> for Mempool {
    fn absorb_first(&mut self, op: &mut MempoolOp, _: &Self) {
        match op {
            MempoolOp::Add(record) => {
                self.pool.insert(record.txn_id.clone(), record.clone());
            },
            MempoolOp::Remove(id) => {
                self.pool.remove(id);
            },
        }
    }

    fn sync_with(&mut self, first: &Self) {
        *self = first.clone();
    }
}

pub trait FetchFiltered {
    fn fetch_pending<F>(&self, amount: u32, f: F) -> Vec<TxnRecord>
    where
        F: FnMut(&String, &mut TxnRecord) -> bool;
}

impl FetchFiltered for ReadHandle<Mempool> {
    fn fetch_pending<F>(&self, amount: u32, f: F) -> Vec<TxnRecord>
    where
        F: FnMut(&String, &mut TxnRecord) -> bool,
    {
        if let Some(map) = self.enter().map(|guard| guard.clone()) {
            let mut result = map.pending;
            result.retain(f);
            let mut returned = Vec::<TxnRecord>::new();
            for (_, v) in &result {
                returned.push(v.clone());
            }
            // TODO:  Error - length
            return returned[0..amount as usize].to_vec();
        };
        Vec::<TxnRecord>::new()
    }
}

#[derive(Debug)]
pub struct LeftRightMempoolDB {
    pub read: ReadHandle<Mempool>,
    pub write: WriteHandle<Mempool, MempoolOp>,
}

impl Default for LeftRightMempoolDB {
    fn default() -> Self {
        let (write, read) = left_right::new::<Mempool, MempoolOp>();

        LeftRightMempoolDB { read, write }
    }
}

impl LeftRightMempoolDB {
    /// Creates new Mempool DB
    pub fn new() -> Self {
        Self::default()
    }

    /// Getter for Mempool DB
    pub fn pool(&self) -> Mempool {
        self.read
            .enter()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Getter for Mempool DB
    pub fn get(&self) -> Option<Mempool> {
        self.read.enter().map(|guard| guard.clone())
    }

    /// Returns a new ReadHandleFactory, to simplify multithread access.
    pub fn factory(&self) -> ReadHandleFactory<Mempool> {
        self.read.factory()
    }

    /// Adds a new transaction, makes sure it is unique in db.
    /// Pushes to the ReadHandle.
    pub fn add_txn(&mut self, txn: &Txn, status: TxnStatus) -> Result<()> {
        self.write
            .append(MempoolOp::Add(TxnRecord::new(txn), status))
            .publish();
        Ok(())
    }

    // TODO: refactor mempool to
    // adapt to the new op type
    // filter by txn status
    // pass tests

    pub fn insert(&mut self, txn: &Txn) -> Result<()> {
        self.write
            .append(MempoolOp::Add(TxnRecord::new(txn), status))
            .publish();
        Ok(())
    }

    /// Retrieves a single transaction identified by id, makes sure it exists in
    /// db. Pushes to the ReadHandle.
    pub fn get_txn(&mut self, txn_id: &String) -> Option<Txn> {
        if txn_id.is_empty() {
            return None;
        }

        self.pool()
            .and_then(|map| map.pending.get(txn_id).map(|t| Txn::from(t.txn)))
    }

    /// Getter for an entire pending Txn record
    pub fn get_txn_record(&mut self, txn_id: &String) -> Option<TxnRecord> {
        if txn_id.is_empty() {
            return None;
        }

        self.get().and_then(|map| map.pending.get(txn_id).cloned())
    }

    /// Getter for an entire validated Txn record
    pub fn get_txn_record_validated(&mut self, txn_id: &String) -> Option<TxnRecord> {
        if txn_id.is_empty() {
            return None;
        }

        self.get()
            .and_then(|map| map.validated.get(txn_id).cloned())
    }

    /// Getter for an entire rejected Txn record
    pub fn get_txn_record_rejected(&mut self, txn_id: &String) -> Option<TxnRecord> {
        if txn_id.is_empty() {
            return None;
        }

        self.get().and_then(|map| map.rejected.get(txn_id).cloned())
    }

    /// Adds a batch of new transaction, makes sure that each is unique in db.
    /// Pushes to ReadHandle after processing of the entire batch.
    ///
    /// # Examples
    /// ```
    /// use std::collections::{HashMap, HashSet};
    ///
    /// use mempool::mempool::{LeftRightMemPoolDB, TxnStatus};
    /// use vrrb_core::{keypair::KeyPair, txn::Txn};
    /// let keypair = KeyPair::random();
    /// let mut lrmempooldb = LeftRightMemPoolDB::new();
    /// let mut txns = HashSet::<Txn>::new();
    ///
    /// txns.insert(Txn {
    ///     txn_id: String::from("1"),
    ///     txn_timestamp: 0,
    ///     sender_address: String::from("aaa1"),
    ///     sender_public_key: keypair.get_miner_public_key().serialize().to_vec(),
    ///     receiver_address: String::from("bbb1"),
    ///     txn_token: None,
    ///     txn_amount: 0,
    ///     txn_payload: String::from("x"),
    ///     txn_signature: String::from("x"),
    ///     validators: HashMap::<String, bool>::new(),
    ///     nonce: 0,
    /// });
    ///
    /// match lrmempooldb.add_txn_batch(&txns, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// assert_eq!(1, lrmempooldb.size().0);
    /// ```
    pub fn add_txn_batch(
        &mut self,
        txn_batch: &HashSet<Txn>,
        txns_status: TxnStatus,
    ) -> Result<()> {
        txn_batch.iter().for_each(|t| {
            self.write
                .append(MempoolOp::Add(TxnRecord::new(t), txns_status.clone()));
        });
        self.publish();
        Ok(())
    }

    pub fn extend(&mut self, txn_batch: &HashSet<StatefulTxnRecord>) -> Result<()> {
        txn_batch.iter().for_each(|t| {
            self.write.append(MempoolOp::Add(StatefulTxnRecord::new(t)));
        });

        self.publish();
        Ok(())
    }

    /// Removes a single transaction identified by id, makes sure it exists in
    /// db. Pushes to the ReadHandle.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::{HashMap, HashSet};
    ///
    /// use mempool::mempool::{LeftRightMemPoolDB, TxnStatus};
    /// use vrrb_core::{keypair::KeyPair, txn::Txn};
    /// let keypair = KeyPair::random();
    /// let mut lrmempooldb = LeftRightMemPoolDB::new();
    /// let mut txns = HashSet::<Txn>::new();
    /// let txn_id = String::from("1");
    ///
    /// txns.insert(Txn {
    ///     txn_id: txn_id.clone(),
    ///     txn_timestamp: 0,
    ///     sender_address: String::from("aaa1"),
    ///     sender_public_key: keypair.get_miner_public_key().serialize().to_vec(),
    ///     receiver_address: String::from("bbb1"),
    ///     txn_token: None,
    ///     txn_amount: 0,
    ///     txn_payload: String::from("x"),
    ///     txn_signature: String::from("x"),
    ///     validators: HashMap::<String, bool>::new(),
    ///     nonce: 0,
    /// });
    ///
    /// match lrmempooldb.add_txn_batch(&txns, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// match lrmempooldb.remove_txn_by_id(txn_id.clone()) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// assert_eq!(0, lrmempooldb.size().0);
    /// ```
    pub fn remove_txn_by_id(&mut self, txn_id: String) -> Result<()> {
        self.write
            .append(MempoolOp::Remove(
                TxnRecord::new_by_id(&txn_id),
                TxnStatus::Pending,
            ))
            .publish();
        Ok(())
    }

    /// Removes a single transaction identified by itself, makes sure it exists
    /// in db. Pushes to the ReadHandle.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::{HashMap, HashSet};
    ///
    /// use mempool::mempool::{LeftRightMemPoolDB, TxnStatus};
    /// use vrrb_core::{keypair::KeyPair, txn::Txn};
    /// let keypair = KeyPair::random();
    /// let mut lrmempooldb = LeftRightMemPoolDB::new();
    /// let txn_id = String::from("1");
    ///
    /// let txn = Txn {
    ///     txn_id: txn_id.clone(),
    ///     txn_timestamp: 0,
    ///     sender_address: String::from("aaa1"),
    ///     sender_public_key: keypair.get_miner_public_key().serialize().to_vec(),
    ///     receiver_address: String::from("bbb1"),
    ///     txn_token: None,
    ///     txn_amount: 0,
    ///     txn_payload: String::from("x"),
    ///     txn_signature: String::from("x"),
    ///     validators: HashMap::<String, bool>::new(),
    ///     nonce: 0,
    /// };
    ///
    /// match lrmempooldb.add_txn(&txn, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    /// match lrmempooldb.remove_txn(&txn, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// assert_eq!(0, lrmempooldb.size().0);
    /// ```
    pub fn remove_txn(&mut self, txn: &Txn, status: TxnStatus) -> Result<()> {
        self.write
            .append(MempoolOp::Remove(TxnRecord::new(txn), status))
            .publish();
        Ok(())
    }

    /// Removes a batch of transactions, makes sure that each is unique in db.
    /// Pushes to ReadHandle after processing of the entire batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::{HashMap, HashSet};
    ///
    /// use mempool::mempool::{LeftRightMemPoolDB, TxnStatus};
    /// use vrrb_core::{keypair::KeyPair, txn::Txn};
    /// let keypair = KeyPair::random();
    /// let mut lrmempooldb = LeftRightMemPoolDB::new();
    /// let mut txns = HashSet::<Txn>::new();
    /// let txn_id = String::from("1");
    ///
    /// txns.insert(Txn {
    ///     txn_id: txn_id.clone(),
    ///     txn_timestamp: 0,
    ///     sender_address: String::from("aaa1"),
    ///     sender_public_key: keypair.get_miner_public_key().serialize().to_vec(),
    ///     receiver_address: String::from("bbb1"),
    ///     txn_token: None,
    ///     txn_amount: 0,
    ///     txn_payload: String::from("x"),
    ///     txn_signature: String::from("x"),
    ///     validators: HashMap::<String, bool>::new(),
    ///     nonce: 0,
    /// });
    ///
    /// match lrmempooldb.add_txn_batch(&txns, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// match lrmempooldb.remove_txn_batch(&txns, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// assert_eq!(0, lrmempooldb.size().0);
    /// ```
    // TODO: fix docs
    pub fn remove_txn_batch(
        &mut self,
        txn_batch: &HashSet<Txn>,
        txns_status: TxnStatus,
    ) -> Result<()> {
        txn_batch.iter().for_each(|t| {
            self.write
                .append(MempoolOp::Remove(TxnRecord::new(t), txns_status.clone()));
        });

        self.publish();

        Ok(())
    }

    /// Apply Txn on debits and credits of currect state
    // TODO: to be clarified against the new state representation.
    #[deprecated]
    pub fn apply_txn_on_state(&mut self, _txn: &Txn) -> Result<()> {
        //
        Ok(())
    }

    /// Was the Txn validated ? And when ?
    pub fn is_txn_validated(&mut self, txn: &Txn) -> Result<TxTimestamp> {
        // if let Some(txn_record_validated) = self.get_txn_record_validated(&txn.txn_id) {
        if let Some(txn_record_validated) = self.get_txn_record_validated(&txn.digest()) {
            Ok(txn_record_validated.txn_validated_timestamp)
        } else {
            Err(MempoolError::TransactionMissing)
        }
    }

    /// Was the Txn rejected ? And when ?
    pub fn is_txn_rejected(&mut self, txn: &Txn) -> Result<TxTimestamp> {
        // if let Some(txn_record_rejected) = self.get_txn_record_rejected(&txn.txn_id) {
        if let Some(txn_record_rejected) = self.get_txn_record_rejected(&txn.digest()) {
            Ok(txn_record_rejected.txn_rejected_timestamp)
        } else {
            Err(MempoolError::TransactionMissing)
        }
    }

    /// Purge rejected transactions.
    pub fn purge_txn_rejected(&mut self) -> Result<()> {
        if let Some(mut map) = self.get() {
            map.rejected.clear()
        }

        Ok(())
    }

    /// Retrieves actual size of the mempooldb.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::{HashMap, HashSet};
    ///
    /// use mempool::mempool::{LeftRightMemPoolDB, TxnStatus};
    /// use vrrb_core::txn::Txn;
    /// let keypair = vrrb_core::keypair::KeyPair::random();
    /// let mut lrmempooldb = LeftRightMemPoolDB::new();
    /// let mut txns = HashSet::<Txn>::new();
    /// let txn_id = String::from("1");
    ///
    /// txns.insert(Txn {
    ///     txn_id: txn_id.clone(),
    ///     txn_timestamp: 0,
    ///     sender_address: String::from("aaa1"),
    ///     sender_public_key: keypair.get_miner_public_key().serialize().to_vec(),
    ///     receiver_address: String::from("bbb1"),
    ///     txn_token: None,
    ///     txn_amount: 0,
    ///     txn_payload: String::from("x"),
    ///     txn_signature: String::from("x"),
    ///     validators: HashMap::<String, bool>::new(),
    ///     nonce: 0,
    /// });
    ///
    /// match lrmempooldb.add_txn_batch(&txns, TxnStatus::Pending) {
    ///     Ok(_) => {},
    ///     Err(_) => {},
    /// };
    ///
    /// assert_eq!(1, lrmempooldb.size().0);
    /// ```
    pub fn size(&self) -> (usize, usize, usize) {
        if let Some(map) = self.get() {
            (map.pending.len(), map.validated.len(), map.rejected.len())
        } else {
            (0, 0, 0)
        }
    }

    /// Pushes changes to Reader.
    fn publish(&mut self) {
        self.write.publish();
    }
}

impl From<Mempool> for LeftRightMempoolDB {
    fn from(pool: Mempool) -> Self {
        let (write, read) = left_right::new::<Mempool, MempoolOp>();
        let mut mempool_db = Self {
            // read: self.read.clone(),
            read,
            write,
        };

        let pending_values = pool.pending.values();
        let rejected_values = pool.rejected.values();
        let validated_values = pool.validated.values();

        let pending_set = pending_values
            .map(|(_, v)| v)
            .collect::<HashSet<TxnRecord>>();

        // let pending_set = HashSet::from(pool.pending.values());
        let rejected_set = HashSet::from(pool.rejected.values());
        let validated_set = HashSet::from(pool.validated.values());

        mempool_db.add_txn_batch(pending_set);

        mempool_db
    }
}

impl Clone for LeftRightMempoolDB {
    fn clone(&self) -> Self {
        Self::from(self.mempool())
    }
}
