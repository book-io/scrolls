use gasket::error::AsWorkError;
use pallas::crypto::hash::Hash;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::{Arc, Mutex};


#[derive(Deserialize)]
pub struct Config {
    pub connection_params: String,
}

#[derive(Deserialize, Clone)]
pub struct PolicyIds {
    pub policy_ids: Vec<String>,
}

pub struct ReducerState {
    policy_ids: Arc<Mutex<Vec<Hash<28>>>>,
    pub(crate) connection_params: Option<String>,
    pub(crate) connection: Option<redis::Connection>,
}


pub(crate) fn strings_to_hashes(pids_str: &Vec<String>) -> Vec<Hash<28>> {
    pids_str
        .into_iter()
        .map(|pid| Hash::<28>::from_str(&pid).expect("invalid policy_id"))
        .collect()
}



impl ReducerState {
    pub fn new(
    ) -> Self {
        ReducerState {
            policy_ids: Arc::new(Mutex::new(vec![])),
            connection_params: None,
            connection: None,
        }
    }

    pub fn connection_params(&mut self,params: String ) -> &mut Self {
        self.connection_params = Some(params);
        self
    }

    fn with_redis(&mut self) -> &mut Self {

        match self.connection {
            Some(_) => {
                self
            },
            None => {
                self.connection = Some(redis::Client::open(self.connection_params.as_mut().unwrap().clone())
                .and_then(|x| x.get_connection())
                .map_err(crate::Error::storage).expect("Redis client couldn't get instanciated"));
                self
            }
        }

    }

    pub fn exist(&mut self, policy_id: &Hash<28>) -> anyhow::Result<bool> {

        let con = self.with_redis()
            .connection
            .as_mut()
            .unwrap();

        let policies: Vec<String> = redis::cmd("smembers").query(con).or_restart()?;
        let hashes = strings_to_hashes(&policies);
        let exist = hashes.iter().any(|hash| policy_id.eq(hash));
        Ok(exist)
    }


    pub fn policy_ids(&mut self) -> Arc<Mutex<Vec<Hash<28>>>> {


        let policies_arc = self.policy_ids.clone();
        let mut policy_lock = policies_arc.lock().unwrap();

        let con = self.with_redis()
            .connection
            .as_mut()
            .unwrap();

        let policies: Vec<String> = redis::cmd("smembers").query(con).unwrap();
        let mut hashes = strings_to_hashes(&policies);
        policy_lock.append(&mut hashes);
        self.policy_ids.clone()
    }

}
