use clap::Parser;

mod cli;
pub(crate) use crate::cli::*;
pub(crate) mod commands;
pub mod result;

#[telemetry::instrument]
pub async fn run() -> anyhow::Result<()> {
    dbg!("in lib.rs run");
    
    let args = Args::parse();

    dbg!("ARGS: {}", &args);

    commands::exec(args).await?;

    Ok(())
}
