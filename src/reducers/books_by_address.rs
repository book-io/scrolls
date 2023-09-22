use bech32::{ToBase32, Variant};
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use notify::{Event, INotifyWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use pallas::ledger::traverse::{Asset, MultiEraOutput};
use pallas::ledger::traverse::{MultiEraBlock, OutputRef};
use serde::Deserialize;
use std::fs;

use crate::{crosscut, model, prelude::*};
use pallas::crypto::hash::Hash;

use crate::crosscut::epochs::block_epoch;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Deserialize, Copy, Clone)]
pub enum AggrType {
    Epoch,
}

#[derive(Deserialize)]
pub struct Config {
    pub key_prefix: Option<String>,
    pub policy_ids_file: Option<String>,
    pub filter: Option<crosscut::filters::Predicate>,
    pub aggr_by: Option<AggrType>,
}

pub struct ReducerState {
    watcher: Option<RecommendedWatcher>,
    config_file: Option<PathBuf>,
    policy_ids: Arc<Mutex<Vec<Hash<28>>>>,
}

pub struct Reducer {
    config: Config,
    policy: crosscut::policies::RuntimePolicy,
    chain: crosscut::ChainWellKnownInfo,
    state: Arc<Mutex<ReducerState>>
}

#[derive(Deserialize, Clone)]
pub struct PolicyIds {
    pub policy_ids: Vec<String>,
}



impl ReducerState {

fn watch_path(&mut self, path: PathBuf, filename: String) {
        if self.watcher.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            //const DELAY: Duration = Duration::from_millis(200);
            let watcher = notify::recommended_watcher(tx).unwrap();
            let path = path.clone();
            let path_string = format!("{}/{}", path.to_str().unwrap_or_default().to_string(), filename);
            let policies_hex_cloned = self.policy_ids.clone();

            std::thread::spawn(move || {
                // block until we get an event

                //fn extract_path(event: notify::Event) -> Vec<PathBuf> {
                    //match event.kind {
                        //EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                            //event.paths
                        //}
                        //_ => vec![],
                    //}
                //}

                while let Ok(event) = rx.recv() {
                    match event {
                        Ok(event) => {
                            if event.kind.is_modify() {

                                dbg!(&event);
                                match load_config(&path_string) {
                                    Ok(new_config) => {
                                        let new_hashes = config_to_hash(&new_config.policy_ids);
                                        let mut phex = policies_hex_cloned.lock().unwrap();
                                        phex.clear();
                                        *phex = new_hashes;

                                        println!("Reloading config.json ");
                                    }
                                    Err(error) => println!("Error reloading config: {:?}", error),
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("error {:?} changed, reload config", e);
                        }
                    }
                }
            });
            self.watcher.replace(watcher);
        }
        if let Some(watcher) = self.watcher.as_mut() {
            watcher
                .watch(&path, notify::RecursiveMode::Recursive)
                .ok();
        }
    }

}

impl Reducer {




    fn is_policy_id_accepted(&self, policy_id: &Hash<28>) -> bool {
        match self.state.clone().lock().unwrap().policy_ids.clone().lock().map(|ids| {
            let pids = ids;
            pids.contains(&policy_id)
        }) {
            Ok(p) => p,
            Err(_) => false,
        }
    }

    fn process_consumed_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        _epoch_no: u64,
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

fn config_to_hash(pids_str: &Vec<String>) -> Vec<Hash<28>> {
    pids_str
        .into_iter()
        .map(|pid| Hash::<28>::from_str(&pid).expect("invalid policy_id"))
        .collect()
}

/*fn watch() -> Result<()>{*/

/*}*/

pub fn load_config(path: &String) -> Result<PolicyIds, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 {
        return Err("The config file is empty.".into());
    }

    let reader = std::io::BufReader::new(file);

    let pids: PolicyIds = serde_json::from_reader(reader)?;
    Ok(pids)
}

impl Config {
    pub fn plugin(
        self,
        chain: &crosscut::ChainWellKnownInfo,
        policy: &crosscut::policies::RuntimePolicy,
    ) -> super::Reducer {

        let path = match self.policy_ids_file {
            Some(ref p) => p.clone(),
            None => "./config.json".to_string(),
        };

        let pids_config = load_config(&path).unwrap();
        let pids_hex = config_to_hash(&pids_config.policy_ids);
        let policy_ids = Arc::new(Mutex::new(pids_hex));

        let p = fs::canonicalize(&path).unwrap().clone();


        let state = Arc::new(Mutex::new(
            ReducerState {
                config_file: Some(p.clone()),
                watcher: None,
                policy_ids
            }
        ));

        state.lock().unwrap().watch_path(p.parent().unwrap().to_path_buf(), p.file_name().unwrap().to_string_lossy().to_string());
        //reducer.watch_path(p.parent().unwra());

        let reducer = Reducer {
            config: self,
            chain: chain.clone(),
            policy: policy.clone(),
            state
        };

        super::Reducer::BookByAddress(reducer)
    }
}
