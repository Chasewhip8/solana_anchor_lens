use anchor_lang::solana_program::pubkey;
use anyhow::Result;
use solana_anchor_lens::AnchorLens;
use solana_client::rpc_client::RpcClient;

fn main() -> Result<()> {
    let client = RpcClient::new("https://api.mainnet-beta.solana.com");
    let deser = AnchorLens::new(client);

    let key = pubkey!("8szGkuLTAux9XMgZ2vtY39jVSowEcpBfFfD8hXSEqdGC");
    println!("Attempting to parse account {}", key.to_string());

    let (program_name, act_type, value) = deser
        .fetch_and_deserialize_account_without_idl(&key)?;
    println!("Found program: {}", program_name);
    println!("Found account type: {}", act_type);
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
