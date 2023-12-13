use anyhow::Result;
use bonsaidb::core::connection::{Connection, StorageConnection};
use bonsaidb::local::Storage;
use bonsaidb::{
    core::schema::{Collection, SerializedCollection},
    local::config::{Builder, StorageConfiguration},
};
use clap::Parser;
use ethereum_types::{Address, H160, U256};

const DEFAULT_BALANCE: &str = "10000";

#[derive(Collection, Default, Clone, Parser, Debug)]
#[collection(name = "test-init-db")]
pub struct TestInitDBOpts {
    /// This is the path to the database to be created/used. #716, this path is what we'll feed
    /// into the database driver.
    #[clap(short, long)]
    pub dbpath: String,
    /// Force DB to be initialised, even if it already exists. #716 I think that if this option is
    /// missing or false, we should only recreate the database from defaults if it doesn't already
    /// exist. If it exists, we should exit with a failure and an error message indicating that the
    /// database already exists and to use --force.
    #[clap(short, long)]
    pub force: Option<bool>,
    /// Default balance for new test accounts created. The protocol supports values up to
    /// [ethnum::U256] in size, but u128 ought to be fine for now.
    #[clap(short, long)]
    pub default_balance: Option<u128>,
}
#[derive(Collection, SerializedCollection, Clone, Parser, Debug)]
#[collection(name = "account-info")]
pub struct AccountInfo {
    /// Address of the smart contract's blockchain account
    pub account_address: Address,
    /// Current balance of the smart contract's account at last block
    pub account_balance: U256,
}

impl SerializedCollection for AccountInfo {}

#[derive(Collection, Clone, Parser, Debug)]
#[collection(name = "protocol-inputs")]
pub struct ProtocolInputs {
    /// The block number/height of the block currently being processed
    pub block_height: u64,
    /// The timestamp of the block currently being processed
    pub block_time: u64,
}

fn main_init() -> Result<(), bonsaidb::core::Error> {
    let storage = Storage::open(
        StorageConfiguration::new("testinit.bonsaidb").with_schema::<TestInitDBOpts>()?,
    )?;
    let account_info = storage.create_database::<AccountInfo>("account-info", true)?;
    let protocol_inputs = storage.create_database::<ProtocolInputs>("protocol-inputs", true)?;

    insert_info(&account_info, "0x0000000000000000000000000000000000000001")?;
    insert_info(&account_info, "0x0000000000000000000000000000000000000002")?;
    // ^^ This seems like a bad way of doing things.
    // Not sure on how to give detailed connection, ie, having seperate connections for
    // each variable from struct that is inputed
    insert_info(&account_info, DEFAULT_BALANCE)?;
    Ok(())
}

fn insert_info<C: Connection>(connection: &C, value: &str) -> Result<(), bonsaidb::core::Error> {
    AccountInfo {
        account_address: H160([u8, 20]),
        account_balance: U256([u64; 4]),
    }
    .push_into(connection)?;
    Ok(())
}

/// Initialises a new database for keeping standalone state typically provided by a blockchain.
/// This allows some standalone testing of smart contracts without needing access to a testnet and
/// can also potentially be integrated into common CI/CD frameworks.
pub fn run(opts: &TestInitDBOpts) -> Result<()> {
    // #716, here we want to create a new database to be used by the rest of the functionality in
    // issue #716. This database could be SQLite3 or similar, but with some caveats:
    //  - The database can be written to a single file
    //  - The database has native Rust drivers (ie, not any kind of C/FFI dependency)
    //  - No other external dependencies (such as a specific binary or library to have to be
    //  installed in order to run)
    //  - Not require a separate database service to be running in the background.
    //  - Hopefully be able to support U256 integers
    //
    //  Given these options, I *believe* that these might be suitable:
    //  * https://github.com/rusqlite/rusqlite
    //  * https://www.structsy.rs/
    //
    //  There may be others too. My guess is that rusqlite is probably going to be the most ideal.
    //
    //  [actually, it looks like rusqlite has a dependency on libsqlite, which
    //  isn't necessarily present on the machines we want to run on. I took a
    //  quick look at BonsaiDB and it looks like a pretty good fit for what we
    //  want. It's not a single file, but does contain everything under a single
    //  directory, which ought to suffice.
    //
    //  When creating the new database, I think we want two tables:
    //
    //  1) accounts, which is a two-column table containing a column for an account address, and a
    //     column for an account balance (to mirror this struct https://github.com/versatus/versatus-rust/blob/main/src/versatus_rust.rs#L83). When creating this table, we should also create 16 sample accounts with the addresses 0x000....[1-f].
    //     We should also assign each a default balance -- either the one specified on the command
    //     line (see option above) or, say, 1000.
    //
    //  2) protocol, which is a two column table with a single row that represents the protocol
    //     inputs struct (https://github.com/versatus/versatus-rust/blob/main/src/versatus_rust.rs#L70).
    //     We need only track the block_height (monotonically incrementing number) and the block time (date stamp).
    //
    //     Anytime the test subcommand is executed, these two fields should be updated. I'll
    //     include details under that subcommand's code.
    Ok(())
}
