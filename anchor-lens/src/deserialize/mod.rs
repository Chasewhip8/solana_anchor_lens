use crate::deserialize::idl_type_deserializer::TypeDefinitionDeserializer;
use crate::fetch_idl::discriminators::IdlWithDiscriminators;
use crate::fetch_idl::fetch_idl;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use solana_account_decoder::{UiAccountData, UiAccountEncoding};
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::account::{Account, ReadableAccount};
use solana_sdk::bs58;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, EncodedTransactionWithStatusMeta, UiCompiledInstruction, UiInstruction, UiTransactionEncoding, UiTransactionStatusMeta};
use std::cell::RefCell;
use std::collections::HashMap;
use solana_program::instruction::CompiledInstruction;
use solana_program::message::VersionedMessage;
use solana_transaction_status::option_serializer::OptionSerializer;
use crate::deserialize::instruction::{AccountMetaGroups, deser_ix_data_from_idl};

pub mod field;
pub mod idl_type_deserializer;
pub mod instruction;

pub struct IdlDeserializedAccount {
    pub program_name: String,
    pub type_name: String,
    pub data: Value,
}

pub struct TransactionAndInnerIx {
    pub message: VersionedMessage,
    pub inner_instructions: HashMap<u8, Vec<CompiledInstruction>>,
}

/// Wraps client calls and optionally caches the IDLs that it fetches.
/// This is the preferred means of fetching on-chain IDLs.
/// It's also an easy entrypoint to deserialize accounts
/// and transactions, although for finer grained control there are
/// separate functions for each step of the process.
///
/// Deserializes accounts and instructions, relying on the help
/// of program IDL accounts. These are found on chain, and they store
/// an Anchor IDL JSON file in compressed form.
pub struct AnchorLens {
    pub client: RpcClient,
    pub idl_cache: RefCell<HashMap<[u8; 32], IdlWithDiscriminators>>,
    pub cache_idls: bool,
}

impl AnchorLens {
    /// Initializes with caching turned off. This will make [AnchorLens::fetch_idl]
    /// make an RPC call on every call.
    pub fn new(client: RpcClient) -> Self {
        Self {
            client,
            idl_cache: RefCell::new(HashMap::new()),
            cache_idls: false,
        }
    }

    /// Initializes with caching turned off. This will make [AnchorLens::fetch_idl]
    /// look up an IDL in a [HashMap] before making an RPC call and caching
    /// the result.
    pub fn new_with_idl_caching(client: RpcClient) -> Self {
        Self {
            client,
            idl_cache: RefCell::new(HashMap::new()),
            cache_idls: true,
        }
    }

    /// Attempt to find and fetch the IDL from an address.
    ///
    /// You can pass in either the program ID,
    /// or the IDL account address itself if you know it.
    pub fn fetch_idl(&self, program_id: &Pubkey) -> Result<IdlWithDiscriminators> {
        // Try to return a cached IDL if self is configured to do so
        if self.cache_idls {
            if let Some(idl) = self.idl_cache.borrow_mut().get(&program_id.to_bytes()) {
                return Ok(idl.clone());
            }
        }
        let idl = fetch_idl(&self.client, program_id)?;
        // Cache the fetched value if self is configured to do so
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
    pub fn get_versioned_transaction(&self, txid: &Signature) -> Result<TransactionAndInnerIx> {
        let tx = self
            .client
            .get_transaction(txid, UiTransactionEncoding::Base64)?;
        let EncodedConfirmedTransactionWithStatusMeta {
            transaction: EncodedTransactionWithStatusMeta { transaction, meta, .. },
            ..
        } = tx;
        let mut inner_instructions = HashMap::new();
        if let Some(UiTransactionStatusMeta {
                        inner_instructions: OptionSerializer::Some(meta),
                        ..
                    }) = meta {
            for inner_ix in meta.into_iter() {
                inner_instructions.insert(
                    inner_ix.index,
                    inner_ix.instructions
                        .into_iter()
                        .map(|ix| {
                            match ix {
                                UiInstruction::Compiled(ix) => Some(
                                    CompiledInstruction {
                                        program_id_index: ix.program_id_index,
                                        accounts: ix.accounts,
                                        data: bs58::decode(ix.data).into_vec().unwrap()
                                    }
                                ),
                                _ => None,
                            }
                        })
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                );
            }
        }
        let transaction = transaction
            .decode()
            .ok_or(anyhow!("Failed to decode transaction"))?;
        Ok(TransactionAndInnerIx { message: transaction.message, inner_instructions })
    }

    /// Useful for repeated lookups. You can reduce RPC calls by calling
    /// [AnchorLens::fetch_idl] just once, using it in many calls to this.
    pub fn fetch_and_deserialize_account(
        &self,
        pubkey: &Pubkey,
        idl: Option<&IdlWithDiscriminators>,
    ) -> Result<IdlDeserializedAccount> {
        let act = self.get_account(pubkey)?;
        let (program_name, (type_name, value)) = if let Some(idl) = idl {
            let program_name = idl.name.clone();
            (program_name, deserialize_account_from_idl(&idl, &act)?)
        } else {
            let idl = self.fetch_idl(&act.owner)?;
            let program_name = idl.name.clone();
            (program_name, deserialize_account_from_idl(&idl, &act)?)
        };
        Ok(IdlDeserializedAccount {
            program_name,
            type_name,
            data: value,
        })
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
    /// Caution: This calls the `fetch_idl` method on every instruction. Caching is advised!
    pub fn deserialize_transaction(&self, tx: TransactionAndInnerIx) -> Result<Value> {
        let mut instructions_deserialized = vec![];
        for (i, ix) in tx.message.instructions()
            .iter()
            .enumerate() {
            // TODO Cut this block out into its own method
            let idx = ix.program_id_index;
            let program_id = tx.message.static_account_keys()[idx as usize];
            let idl = self.fetch_idl(&program_id);
            if let Ok(idl) = idl {
                let json = self.deserialize_instruction(
                    &idl,
                    i,
                    ix,
                    &tx.message,
                    tx.inner_instructions.get(&u8::try_from(i).unwrap())
                );
                instructions_deserialized.push(json);
            } else {
                let mut inner_ix = vec![];
                if let Some(instructions) = tx.inner_instructions.get(&u8::try_from(i).unwrap()) {
                    for (i, ix) in instructions.iter().enumerate() {
                        let idx = ix.program_id_index;
                        let program_id = tx.message.static_account_keys()[idx as usize];
                        let idl = self.fetch_idl(&program_id);
                        if let Ok(idl) = idl {
                            let json = self.deserialize_instruction(
                                &idl,
                                i,
                                ix,
                                &tx.message,
                                None,
                            );
                            inner_ix.push(json);
                        } else {
                            let json = json!({
                           "program_id": program_id.to_string(),
                           "unknown_ix": format!("instruction {}", i)
                        });
                            inner_ix.push(json);
                        }
                    }
                }
                let mut json = json!({
                   "program_id": program_id.to_string(),
                   "unknown_ix": format!("instruction {}", i)
                });
                if !inner_ix.is_empty() {
                    json.as_object_mut().unwrap().insert(
                        "inner_instructions".to_string(), Value::Array(inner_ix)
                    );
                }
                instructions_deserialized.push(json);
            }
        }
        Ok(Value::Array(instructions_deserialized))
    }

    /// Attempts deserialization of a given transaction instruction.
    /// The [VersionedMessage] passed in must be from the same transaction.
    /// If the attempt fails, we return a JSON object indicating the
    /// reason for failure, and any other information.
    pub fn deserialize_instruction(&self,
        idl: &IdlWithDiscriminators,
        instruction_num: usize,
        ix: &CompiledInstruction,
        message: &VersionedMessage,
        inner_instructions: Option<&Vec<CompiledInstruction>>,
    ) -> Value {
        let idx = ix.program_id_index;
        let program_id = message.static_account_keys()[idx as usize];
        let maybe_deserialized = deser_ix_data_from_idl(&idl, ix.data.clone());
        if let Ok((idl_ix, ix_data)) = maybe_deserialized {
            let account_metas = {
                let mut metas: Vec<Value> = vec![];
                let mut increment: usize = 0;
                let account_meta_groups =
                    AccountMetaGroups::new_from_message(message.clone(), ix.accounts.clone());
                account_meta_groups.idl_accounts_to_json(
                    &mut increment,
                    idl_ix.accounts.clone(),
                    &mut metas,
                );
                metas
            };
            let mut inner_ix = vec![];
            if let Some(instructions) = inner_instructions {
                for (i, ix) in instructions.iter().enumerate() {
                    let idx = ix.program_id_index;
                    let program_id = message.static_account_keys()[idx as usize];
                    let idl = self.fetch_idl(&program_id);
                    if let Ok(idl) = idl {
                        let json = self.deserialize_instruction(
                            &idl,
                            i,
                            ix,
                            &message,
                            None,
                        );
                        inner_ix.push(json);
                    } else {
                        let json = json!({
                           "program_id": program_id.to_string(),
                           "unknown_ix": format!("instruction {}", i)
                        });
                        inner_ix.push(json);
                    }
                }
            }
            let mut output = json!({
               "program_id": program_id.to_string(),
               "program_name": idl.name,
               "instruction": {
                   "name": idl_ix.name,
                   "data": ix_data,
                   "accounts": account_metas
                }
            });
            if !inner_ix.is_empty() {
                output.as_object_mut().unwrap().insert(
                    "inner_instructions".to_string(), Value::Array(inner_ix)
                );
            }
            output
        } else {
            // TODO Maybe add account metas and raw ix data?
            json!({
               "program_id": program_id.to_string(),
               "unknown_discriminator": format!("instruction {}", instruction_num)
            })
        }
    }
}

/// Assuming one already has fetched the account, this method is available,
/// which performs just the deserialization attempt based on an IDL.
/// Returns a tuple of the account type name, and its deserialized
/// data encoded as a [serde_json::Value].
pub fn deserialize_account_from_idl(
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
pub fn deserialized_account_json(
    idl: &IdlWithDiscriminators,
    address: &Pubkey,
    account: Account,
) -> Result<Value> {
    let (account_type, deserialized) = deserialize_account_from_idl(idl, &account)?;
    Ok(json!({
        "pubkey": address.to_string(),
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
