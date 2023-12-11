use std::{collections::HashMap, path::PathBuf};

use anyhow::{anyhow, Result};
use clap::Parser;
use telemetry::info;
use wasm_runtime::{
    metering::{cost_function, MeteringConfig},
    wasm_runtime::WasmRuntime,
};
use wasmer::{Cranelift, Target};

#[derive(Parser, Debug)]
pub struct TestOpts {
    /// This is the path to the database to be created/used. #716, this path is what we'll feed
    /// into the database driver.
    #[clap(short, long)]
    pub dbpath: String,
    /// The path to the WASM object file to load and describe
    #[clap(short, long, value_parser, value_name = "FILE")]
    pub wasm: PathBuf,
    /// The function to call within the smart contract. #716 this will influence the JSON we
    /// generate below to pass into the smart contract when we execute it. TODO: mg@ needs to also
    /// remember to add some function-specific arguments here to allow those to be passed in.
    #[clap(short, long, value_parser, value_name = "FUNCTION")]
    pub function: String,
    /// The arguments to pass into the function as a JSON object. See the `versatus-rust` github
    /// repository for the inputs that supported functions take. For now, this is a string
    /// interpretted as a JSON object, whereas later, it'll likely be more formal. #716, this JSON
    /// will equate to the data in the FunctionInputs enum here:
    /// https://github.com/versatus/versatus-rust/blob/main/src/versatus_rust.rs#L94
    #[clap(short, long, value_parser, value_name = "JSON", default_value = "[]")]
    pub inputs: String,
    /// An environment variable to pass to the running WASM module. May be used
    /// multiple times.
    #[clap(short, long, value_parser, value_name = "KEY=VALUE")]
    pub env: Vec<String>,
    /// The initial limit of credits that the WASM module's meter will use to track
    /// operation expenses.
    #[clap(short = 'l', long, value_parser, value_name = "UINT64")]
    pub meter_limit: u64,
    /// Remaining arguments (after '--') are passed to the WASM module command
    /// line.
    #[clap(last = true)]
    pub args: Vec<String>,
}

/// Read and parse a WASM object and print high level information that is
/// targeted toward developers of WASM modules. It should attempt to describe
/// how the module might, or might not, be viable as an off-chain smart contract
/// compute job.
pub fn run(opts: &TestOpts) -> Result<()> {
    let wasmfile = opts
        .wasm
        .to_str()
        .ok_or(anyhow!("Failed to convert WASM filename to valid string."))?;
    let wasm_bytes = std::fs::read(wasmfile)?;
    info!(
        "Loaded {} bytes of WASM data from {} to execute.",
        wasm_bytes.len(),
        wasmfile
    );
    // #716 This JSON string is a placeholder to allow the code to compile. What we need to do is
    // to build the JSON to match the JSON generated by the example in the versatus-rust
    // repository, but build it from the contents of the database and command line inputs. We can
    // assume (for now) that all contracts will be ERC20 when dealing with inputs and outputs.
    let json_data = "{ \"replace\": \"me\" }".as_bytes();

    let mut env_vars: HashMap<String, String> = HashMap::new();
    for var in opts.env.iter() {
        if let Some((key, value)) = var.split_once('=') {
            env_vars.insert(key.to_string(), value.to_string());
        }
    }

    let target = Target::default();
    // Test the WASM module.
    let mut wasm = WasmRuntime::new::<Cranelift>(
        &target,
        &wasm_bytes,
        MeteringConfig::new(opts.meter_limit, cost_function),
    )?
    .stdin(&json_data)?
    .env(&env_vars)?
    .args(&opts.args)?;
    wasm.execute()?;

    // #716 We shouldn't print the output here, but rather parse it and use it to update the
    // database. For example, if an ErcTransferEvent is part of the output(https://github.com/versatus/versatus-rust/blob/main/src/eip20.rs#L48), we should move the balance from the from account to the to account.
    println!("{}", &wasm.stdout());

    eprintln!("Contract errors: {}", &wasm.stderr());

    Ok(())
}
