use std::fs::File;
use std::io::Write;
use anyhow::Result;
use clap::Parser;
use jungle_fi_cli_utils::clap::{pubkey_or_signer_path, UrlArg, pubkey_arg};
use serde_json::json;
use solana_client::rpc_client::RpcClient;
use solana_sdk::bs58;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_anchor_lens::AnchorLens;
use solana_anchor_lens::deserialize::deserialized_account_json;


/// Account data cloning CLI.
#[derive(Debug, Parser)]
pub struct Opts {
    #[clap(flatten)]
    pub url: UrlArg,
    #[clap(subcommand)]
    pub command: Command,
}


#[derive(Debug, Parser)]
pub enum Command {
    Account {
        #[clap(parse(try_from_str=pubkey_arg))]
        address: Pubkey,
        #[clap(short, long)]
        outfile: Option<String>,
    },
}

pub fn entry(
    opts: &Opts,
    client: RpcClient,
) -> Result<()> {
    match &opts.command {
        Command::Account { address, outfile } => {
            let lens = AnchorLens::new(client);
            let account = lens.get_account(address)?;
            let idl = lens.get_idl(&account.owner)?;
            let json = deserialized_account_json(&idl, address, account)?;
            let json = serde_json::to_string_pretty(&json)?;
            if let Some(outfile) = outfile {
                let mut file = File::create(outfile)?;
                file.write(json.as_bytes())?;
            }
        }
    }
    Ok(())
}