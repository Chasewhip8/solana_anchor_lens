use crate::deserialize::idl_type_deserializer::TypeDefinitionDeserializer;
use crate::fetch_idl::discriminators::IdlWithDiscriminators;
use crate::fetch_idl::fetch_idl;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use solana_account_decoder::{UiAccountData, UiAccountEncoding};
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::account::Account;
use solana_sdk::bs58;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransactionWithStatusMeta,
    UiTransactionEncoding,
};
use std::cell::RefCell;
use std::collections::HashMap;

pub mod field;
pub mod idl_type_deserializer;
pub mod instruction;

/// Deserializes accounts and instructions, relying on the help
/// of program IDL accounts. These are found on chain, and they store
/// an Anchor IDL JSON file in compressed form.
pub struct AnchorLens {
    pub client: RpcClient,
    pub idl_cache: RefCell<HashMap<[u8; 32], IdlWithDiscriminators>>,
    pub cache_idls: bool,
}

impl AnchorLens {
    /// Initializes with caching turned off. This will make [AnchorLens::get_idl]
    /// make an RPC call on every call.
    pub fn new(client: RpcClient) -> Self {
        Self {
            client,
            idl_cache: RefCell::new(HashMap::new()),
            cache_idls: false,
        }
    }

    /// Initializes with caching turned off. This will make [AnchorLens::get_idl]
    /// look up an IDL in a [HashMap] before making an RPC call and caching
    /// the result.
    pub fn new_with_idl_caching(client: RpcClient) -> Self {
        Self {
            client,
            idl_cache: RefCell::new(HashMap::new()),
            cache_idls: true,
        }
    }

    /// Returns a tuple of the account type name, and the deserialized JSON data.
    ///
    /// This method is a good one-liner with a simple argument signature
    /// for account fetch and deserialization, but it's needlessly costly
    /// if you're going to deserialize multiple accounts.
    ///
    /// You can save on RPC calls by calling [AnchorLens::get_idl], saving the resultant
    /// [crate::fetch_idl::discriminators::IdlWithDiscriminators] instance, and calling
    /// [AnchorLens::fetch_and_deserialize_account].
    pub fn fetch_and_deserialize_account_without_idl(
        &self,
        pubkey: &Pubkey,
    ) -> Result<(String, String, Value)> {
        let act = self.get_account(pubkey)?;
        let idl = self.get_idl(&act.owner)?;
        let (ix_name, value) = self.deserialize_account_from_idl(&idl, &act)?;
        Ok((idl.name.clone(), ix_name, value))
    }

    /// Attempt to find and fetch the IDL from an address.
    ///
    /// You can pass in either the program ID,
    /// or the IDL account address itself if you know it.
    pub fn get_idl(&self, program_id: &Pubkey) -> Result<IdlWithDiscriminators> {
        if self.cache_idls {
            if let Some(idl) = self.idl_cache.borrow_mut().get(&program_id.to_bytes()) {
                return Ok(idl.clone());
            }
        }
        let idl = fetch_idl(&self.client, program_id)?;
        if self.cache_idls {
            self.idl_cache
                .borrow_mut()
                .insert(program_id.to_bytes(), idl.clone());
        }
        Ok(idl)
    }

    /// Convenience function, uses `self.client` to fetch the [solana_sdk::account::Account], unserialized.
    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        Ok(self.client.get_account(pubkey)?)
    }

    /// Fetches a historical transaction (the message and its signatures), filtering out
    /// the rest of the usual `get_transaction` RPC response.
    pub fn get_versioned_transaction(&self, txid: &Signature) -> Result<VersionedTransaction> {
        let tx = self
            .client
            .get_transaction(txid, UiTransactionEncoding::Base64)?;
        let EncodedConfirmedTransactionWithStatusMeta {
            transaction: EncodedTransactionWithStatusMeta { transaction, .. },
            ..
        } = tx;
        Ok(transaction
            .decode()
            .ok_or(anyhow!("Failed to decode transaction"))?)
    }

    /// Useful for repeated lookups. You can reduce RPC calls by calling
    /// [AnchorLens::get_idl] just once, using it in many calls to this.
    pub fn fetch_and_deserialize_account(
        &self,
        idl: &IdlWithDiscriminators,
        pubkey: &Pubkey,
    ) -> Result<(String, Value)> {
        let act = self.get_account(pubkey)?;
        Ok(self.deserialize_account_from_idl(&idl, &act)?)
    }

    /// Assuming one already has fetched the account, this method is available,
    /// which performs just the deserialization attempt based on an IDL.
    /// Returns a tuple of the account type name, and its deserialized
    /// data encoded as a [serde_json::Value].
    pub fn deserialize_account_from_idl(
        &self,
        idl: &IdlWithDiscriminators,
        account: &Account,
    ) -> Result<(String, Value)> {
        let idl_type_defs = idl.types.clone();
        let mut first_eight = account.data.to_vec();
        first_eight.resize(8, 0);
        let first_eight: [u8; 8] = first_eight.try_into().unwrap();
        let type_def = idl
            .discriminators
            .accounts
            .get(&first_eight)
            .ok_or(anyhow!(
                "Could not match account data against any discriminator"
            ))?;
        Ok((
            (type_def.name.clone()),
            TypeDefinitionDeserializer {
                idl_type_defs,
                curr_type: type_def.clone(),
            }
            .deserialize(&mut account.data.as_slice())?,
        ))
    }

    /// Fetches the account data, attempts to deserialize it, and returns
    /// a JSON value compatible with `solana-test-validator --account` JSON files,
    /// but with additional fields that store deserialized account data. The extra
    /// fields do not interfere with using these values for localnet testing.
    pub fn descriptive_ui_account_json(
        &self,
        idl: &IdlWithDiscriminators,
        pubkey: &Pubkey,
    ) -> Result<Value> {
        let account = self.client.get_account(pubkey)?;
        let (account_type, deserialized) = self.deserialize_account_from_idl(idl, &account)?;
        Ok(json!({
            "pubkey": pubkey.to_string(),
            "account": {
                "data": UiAccountData::Binary(
                        bs58::encode(&account.data).into_string(),
                        UiAccountEncoding::Base58,
                    ),
                "lamports": account.lamports,
                "owner": account.owner.to_string(),
                "executable": account.executable,
                "rent_epoch": account.rent_epoch,
            },
            "program_name": idl.name.clone(),
            "account_type": account_type,
            "deserialized": deserialized,
        }))
    }

    /// Deserializes a transaction's instructions.
    ///
    /// Provides instruction names, deserialized args, and decoded / validated
    /// account metas.
    ///
    /// Regarding validation -- if the transaction message differs
    /// from what the IDL stipulates (i.e. there's an account that is erroneously not
    /// marked mutable), this will flag it with an appropriate
    /// [crate::deserialize::instruction::AccountMetaStatus] variant.
    ///
    /// Caution: This calls the `get_idl` method on every instruction. Caching is advised!
    pub fn deserialize_transaction(&self, tx: VersionedTransaction) -> Result<Value> {
        let mut instructions_deserialized = vec![];
        for (i, ix) in tx.message.instructions().iter().enumerate() {
            let idx = ix.program_id_index;
            let program_id = tx.message.static_account_keys()[idx as usize];
            let idl = self.get_idl(&program_id);
            if let Ok(idl) = idl {
                let json = instruction::deserialize_instruction(&idl, i, ix, &tx.message);
                instructions_deserialized.push(json);
            } else {
                // TODO Maybe add account metas and raw ix data?
                let json = json!({
                   "program_id": program_id.to_string(),
                   "unknown_ix": format!("instruction {}", i)
                });
                instructions_deserialized.push(json);
            }
        }
        Ok(Value::Array(instructions_deserialized))
    }
}
