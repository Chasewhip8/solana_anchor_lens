use anyhow::Result;
use solana_anchor_lens::AnchorLens;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use std::str::FromStr;

fn main() -> Result<()> {
    let client = RpcClient::new("https://api.devnet.solana.com");
    let deser = AnchorLens::new(client);

    let sig = Signature::from_str(
        "4rpGWDteDqd7TSpRMwmRc2G6yq4qCQvbP2b2SF4cgZPg4Ls5shoTEJmyw6Fsy8iApey3YG5NsTrdwrMTtztTUjar",
    )?;
    println!("Attempting to parse transaction {}", sig.to_string());

    // This is the same as `self.client.get_transaction`, but with some preset
    // configuration and verbose unpacking of the RPC response.
    let tx = deser.get_versioned_transaction(&sig)?;

    let deserialized = deser.deserialize_transaction(tx)?;
    println!("{}", serde_json::to_string_pretty(&deserialized)?);
    Ok(())
}
