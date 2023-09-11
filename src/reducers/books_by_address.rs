use bech32::{ToBase32, Variant};
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use pallas::ledger::traverse::{Asset, MultiEraOutput};
use pallas::ledger::traverse::{MultiEraBlock, OutputRef};
use serde::Deserialize;

use crate::{crosscut, model, prelude::*};
use pallas::crypto::hash::Hash;

use crate::crosscut::epochs::block_epoch;
use std::str::FromStr;

#[derive(Deserialize, Copy, Clone)]
pub enum AggrType {
    Epoch,
}

#[derive(Deserialize)]
pub struct Config {
    pub key_prefix: Option<String>,
    pub filter: Option<crosscut::filters::Predicate>,
    pub aggr_by: Option<AggrType>,

    /// Policies to match
    ///
    /// If specified only those policy ids as hex will be taken into account, if
    /// not all policy ids will be indexed.
    pub policy_ids_hex: Option<Vec<String>>,
}

pub struct Reducer {
    config: Config,
    policy: crosscut::policies::RuntimePolicy,
    chain: crosscut::ChainWellKnownInfo,
    policy_ids: Option<Vec<Hash<28>>>,
}

impl Reducer {
    fn config_key(&self, subject: String, epoch_no: u64) -> String {
        let def_key_prefix = "asset_holders_by_asset_id";

        match &self.config.aggr_by {
            Some(aggr_type) if matches!(aggr_type, AggrType::Epoch) => {
                return match &self.config.key_prefix {
                    Some(prefix) => format!("{}.{}.{}", prefix, subject, epoch_no),
                    None => format!("{}.{}", def_key_prefix.to_string(), subject),
                };
            }
            _ => {
                return match &self.config.key_prefix {
                    Some(prefix) => format!("{}.{}", prefix, subject),
                    None => format!("{}.{}", def_key_prefix.to_string(), subject),
                };
            }
        };
    }

    fn is_policy_id_accepted(&self, policy_id: &Hash<28>) -> bool {
        return match &self.policy_ids {
            Some(pids) => pids.contains(&policy_id),
            None => true,
        };
    }

    fn process_consumed_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        epoch_no: u64,
        output: &mut super::OutputPort,
    ) -> Result<(), gasket::error::Error> {
        let utxo = ctx.find_utxo(input).apply_policy(&self.policy).or_panic()?;

        let utxo = match utxo {
            Some(x) => x,
            None => return Ok(()),
        };

        //address will be hash key
        let address = utxo.address().map(|addr| addr.to_string()).or_panic()?;

        for asset in utxo.assets() {
            match asset {
                Asset::NativeAsset(policy_id, name, _quantity) => {
                    if self.is_policy_id_accepted(&policy_id) {
                        let name_str = String::from_utf8(name).unwrap_or_default();
                        let asset_fingerprint = self
                            .asset_fingerprint([
                                policy_id.to_string().as_str(),
                                hex::encode(name_str).as_str(),
                            ])
                            .unwrap_or_default();

                        let crdt = model::CRDTCommand::SetAdd(
                            format!("{}.{}", "BookByAddress".to_string(), address.to_string()),
                            asset_fingerprint,
                        );

                        output.send(gasket::messaging::Message::from(crdt))?;
                    }
                }
                _ => (),
            };
        }

        Ok(())
    }

    fn process_produced_txo(
        &mut self,
        tx_output: &MultiEraOutput,
        _epoch_no: u64,
        output: &mut super::OutputPort,
    ) -> Result<(), gasket::error::Error> {
        let address = tx_output
            .address()
            .map(|addr| addr.to_string())
            .or_panic()?;

        for asset in tx_output.assets() {
            match asset {
                Asset::NativeAsset(policy_id, name, _quantity) => {
                    if self.is_policy_id_accepted(&policy_id) {
                        let name_str = String::from_utf8(name).unwrap_or_default();
                        let asset_fingerprint = self
                            .asset_fingerprint([
                                policy_id.to_string().as_str(),
                                hex::encode(name_str).as_str(),
                            ])
                            .unwrap_or_default();
                        let crdt = model::CRDTCommand::SetAdd(
                            format!("{}.{}", "BookByAddress".to_string(), address.to_string()),
                            asset_fingerprint,
                        );

                        output.send(gasket::messaging::Message::from(crdt))?;
                    }
                }
                _ => {}
            };
        }

        Ok(())
    }
    fn asset_fingerprint(&self, data_list: [&str; 2]) -> Result<String, bech32::Error> {
        let combined_parts = data_list.join("");
        let raw = hex::decode(combined_parts);

        let mut hasher = Blake2bVar::new(20).unwrap();
        hasher.update(&raw.unwrap());
        let mut buf = [0u8; 20];
        hasher.finalize_variable(&mut buf).unwrap();
        let base32_combined = buf.to_base32();
        bech32::encode("asset", base32_combined, Variant::Bech32)
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        output: &mut super::OutputPort,
    ) -> Result<(), gasket::error::Error> {
        for tx in block.txs().into_iter() {
            if filter_matches!(self, block, &tx, ctx) {
                let epoch_no = block_epoch(&self.chain, block);

                for consumed in tx.consumes().iter().map(|i| i.output_ref()) {
                    self.process_consumed_txo(&ctx, &consumed, epoch_no, output)?;
                }

                for (_, meo) in tx.produces() {
                    self.process_produced_txo(&meo, epoch_no, output)?;
                }
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(
        self,
        chain: &crosscut::ChainWellKnownInfo,
        policy: &crosscut::policies::RuntimePolicy,
    ) -> super::Reducer {
        let policy_ids: Option<Vec<Hash<28>>> = match &self.policy_ids_hex {
            Some(pids) => {
                let ps = pids
                    .iter()
                    .map(|pid| Hash::<28>::from_str(pid).expect("invalid policy_id"))
                    .collect();

                Some(ps)
            }
            None => None,
        };

        let reducer = Reducer {
            config: self,
            chain: chain.clone(),
            policy: policy.clone(),
            policy_ids: policy_ids.clone(),
        };

        super::Reducer::BookByAddress(reducer)
    }
}
