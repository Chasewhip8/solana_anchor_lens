use clap::Parser;
use jungle_fi_cli_utils::cli::get_solana_cli_config;
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use crate::interface::{entry, Opts};

mod interface;

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    // Resolve RPC URL from Solana CLI config, or what's passed in.
    let config = get_solana_cli_config()?;
    let url = opts.url.resolve(Some(&config))?;
    let client = RpcClient::new_with_commitment(
        &url,CommitmentConfig::processed());
    entry(
        &opts,
        client
    )
}
