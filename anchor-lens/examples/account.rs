use anchor_lang::solana_program::pubkey;
use anyhow::Result;
use solana_anchor_lens::AnchorLens;
use solana_client::rpc_client::RpcClient;
use solana_anchor_lens::deserialize::IdlDeserializedAccount;

fn main() -> Result<()> {
    let client = RpcClient::new("https://api.mainnet-beta.solana.com");
    let deser = AnchorLens::new(client);

    let key = pubkey!("8szGkuLTAux9XMgZ2vtY39jVSowEcpBfFfD8hXSEqdGC");
    println!("Attempting to parse account {}", key.to_string());

    let IdlDeserializedAccount { program_name, type_name, data } = deser
        .fetch_and_deserialize_account(&key, None)?;
    println!("Found program: {}", program_name);
    println!("Found account type: {}", type_name);
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}
