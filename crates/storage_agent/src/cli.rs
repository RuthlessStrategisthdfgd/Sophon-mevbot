use clap::{Parser, Subcommand};

use crate::commands::daemon::DaemonOpts;
use crate::commands::data_retrieval::DataRetrievalOpts;
use crate::commands::pin_object::PinObjectOpts;
use crate::commands::pin_status::PinStatusOpts;
use crate::commands::status::StatusOpts;

#[derive(Parser)]
#[clap(author, version, about)]
pub struct StorageCli {
    /// The path to the service configuration JSON file.
    #[clap(
        short,
        long,
        value_parser,
        value_name = "FILENAME",
        default_value = "./services.json"
    )]
    pub config: String,
    /// The name of the storage service.
    #[clap(
        short,
        long,
        value_parser,
        value_name = "SERVICE",
        default_value = "default"
    )]
    pub service: String,
    /// The type of service definition to look up.
    #[clap(short = 't', long, value_parser, value_name = "TYPE")]
    pub service_type: Option<String>,
    /// CLI subcommand
    #[clap(subcommand)]
    pub cmd: Option<StorageCommands>,
}

#[derive(Subcommand)]
pub enum StorageCommands {
    /// Starts the storage agent daemon
    Daemon(DaemonOpts),
    /// Shows status of a running agent
    Status(StatusOpts),
    /// Get the object or the Data
    RetrievalData(DataRetrievalOpts),
    /// Pin the object or the Data
    PinObject(PinObjectOpts),
    /// Check the pin status of the object or the Data
    CheckPinStatus(PinStatusOpts),
}
