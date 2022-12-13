use std::fs::File;
use std::io::Write;
use std::str::FromStr;
use anyhow::Result;
use clap::Parser;
use jungle_fi_cli_utils::clap::{UrlArg, pubkey_arg};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_anchor_lens::AnchorLens;
use solana_anchor_lens::deserialize::deserialized_account_json;


/// Account data cloning CLI.
#[derive(Debug, Parser)]
pub struct Opts {
    /// RPC URL to target the Solana cluster
    #[clap(flatten)]
    pub url: UrlArg,
    #[clap(subcommand)]
    pub command: Command,
}


#[derive(Debug, Parser)]
pub enum Command {
    /// Try to deserialize an account, and optionally dump the output to a file.
    Account {
        /// Address of the account to deserialize.
        #[clap(parse(try_from_str=pubkey_arg))]
        address: Pubkey,
        /// Optional output filepath.
        #[clap(short, long)]
        outfile: Option<String>,
    },
    /// Try to deserialize a historical transaction, and optionally dump the output to a file.
    Transaction {
        /// The transaction signature of the historical transaction.
        signature: String,
        /// Optional output filepath.
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
            let idl = lens.fetch_idl(&account.owner)?;
            let json = deserialized_account_json(&idl, address, account)?;
            let json = serde_json::to_string_pretty(&json)?;
            if let Some(outfile) = outfile {
                let mut file = File::create(outfile)?;
                file.write(json.as_bytes())?;
            } else {
                println!("{}", json);
            }
        }
        Command::Transaction { signature, outfile } => {
            let signature = Signature::from_str(signature)?;
            let lens = AnchorLens::new(client);
            let tx = lens.get_versioned_transaction(&signature)?;
            let json = lens.deserialize_transaction(tx)?;
            let json = serde_json::to_string_pretty(&json)?;
            if let Some(outfile) = outfile {
                let mut file = File::create(outfile)?;
                file.write(json.as_bytes())?;
            } else {
                println!("{}", json);
            }
        }
    }
    Ok(())
}