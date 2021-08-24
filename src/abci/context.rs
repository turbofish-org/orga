use std::collections::HashMap;
use std::lazy::SyncLazy;
use std::sync::Mutex;

use tendermint_proto::crypto::{public_key::Sum, PublicKey};

#[derive(Clone, Default)]
pub struct Context {
    pub height: u64,
    pub header: Option<tendermint_proto::types::Header>,

    pub(super) validator_updates: HashMap<[u8; 32], tendermint_proto::abci::ValidatorUpdate>,
}

impl Context {
    pub fn set_voting_power(&self, pub_key: [u8; 32], power: u64) {
        let mut ctx = CONTEXT.lock().unwrap();
        let sum = Some(Sum::Ed25519(pub_key.to_vec()));
        let key = PublicKey { sum };
        ctx.validator_updates.insert(
            pub_key,
            tendermint_proto::abci::ValidatorUpdate {
                pub_key: Some(key),
                power: power as i64,
            },
        );
    }
}

pub static CONTEXT: SyncLazy<Mutex<Context>> = SyncLazy::new(|| Mutex::new(Default::default()));
