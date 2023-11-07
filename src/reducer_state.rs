use notify::{RecommendedWatcher, Watcher};
use pallas::crypto::hash::Hash;
use serde::Deserialize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

#[derive(Deserialize, Clone)]
pub struct PolicyIds {
    pub policy_ids: Vec<String>,
}

pub struct ReducerState {
    watcher: Option<RecommendedWatcher>,
    pub(crate) config_file: Option<PathBuf>,
    pub(crate) policy_ids: Arc<Mutex<Vec<Hash<28>>>>,
}

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

pub(crate) fn config_to_hash(pids_str: &Vec<String>) -> Vec<Hash<28>> {
    pids_str
        .into_iter()
        .map(|pid| Hash::<28>::from_str(&pid).expect("invalid policy_id"))
        .collect()
}

impl ReducerState {
    pub fn new(
        config_file: Option<PathBuf>,
        watcher: Option<RecommendedWatcher>,
        policy_ids: Arc<Mutex<Vec<Hash<28>>>>,
    ) -> Self {
        ReducerState {
            config_file,
            watcher: None,
            policy_ids,
        }
    }

    pub fn watch_path(&mut self, path: PathBuf, filename: String) -> anyhow::Result<()> {
        if self.watcher.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            //const DELAY: Duration = Duration::from_millis(200);
            let watcher = notify::recommended_watcher(tx)?;
            let path = path.clone();
            let path_string = format!(
                "{}/{}",
                path.to_str().unwrap_or_default().to_string(),
                filename
            );
            let policies_hex_cloned = self.policy_ids.clone();

            std::thread::spawn(move || {
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
                                //dbg!(&event);
                                match load_config(&path_string) {
                                    Ok(new_config) => {
                                        let new_hashes = config_to_hash(&new_config.policy_ids);
                                        let mut phex = policies_hex_cloned.lock().unwrap();
                                        phex.clear();
                                        *phex = new_hashes;
                                        //println!("Reloading config.json ");
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
            watcher.watch(&path, notify::RecursiveMode::Recursive).ok();
        }

        Ok(())
    }
}
