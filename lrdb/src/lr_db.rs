use std::{hash::Hash, time::SystemTime};

// pub type Nonce = u32;

/// Struct representing the LeftRight Database.
///
/// `ReadHandleFactory` provides a way of creating new ReadHandles to the database.
///
/// `WriteHandles` provides a way to gain write access to the database.
/// `last_refresh` denotes the lastest `refresh` of the database.
#[allow(dead_code)]
pub struct LeftRightDatabase<K, V>
where
    K: Clone + Eq + Hash + std::fmt::Debug,
    V: Clone + Eq + evmap::ShallowCopy + std::fmt::Debug,
{
    r: evmap::ReadHandleFactory<K, V, ()>,
    w: evmap::WriteHandle<K, V, ()>,
    last_refresh: std::time::SystemTime,
}

impl<K, V> LeftRightDatabase<K, V>
where
    K: Clone + Eq + Hash + std::fmt::Debug,
    V: Clone + Eq + evmap::ShallowCopy + std::fmt::Debug,
{
    pub fn new() -> Self {
        Self::default()
    }
}

impl<K, V> Default for LeftRightDatabase<K, V>
where
    K: Clone + Eq + Hash + std::fmt::Debug,
    V: Clone + Eq + evmap::ShallowCopy + std::fmt::Debug,
{
    fn default() -> Self {
        let (vrrbdb_reader, mut vrrbdb_writer) = evmap::new();
        // TODO: revisit to figure out if this is really necessary
        // This is required to set up oplog
        // Otherwise there's no way to keep track of already inserted keys (before refresh)
        vrrbdb_writer.refresh();
        Self {
            r: vrrbdb_reader.factory(),
            w: vrrbdb_writer,
            last_refresh: SystemTime::now(),
        }
    }
}
