use crate::coins::{Address, Amount, Decimal};
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::LengthVec;
use crate::migrate::MigrateFrom;
use crate::orga;
use crate::plugins::{Signer, Time, ValidatorEntry, Validators};
use crate::prelude::{Read, Store};
use crate::{Error as OrgaError, Result};
use std::collections::HashMap;
use thiserror::Error;

pub const VERSION_KEY: &[u8] = b"/version";

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "Node is running version {expected:?}, but the network has upgraded to version {actual:?}"
    )]
    Version { expected: Version, actual: Version },
}

type PubKey = [u8; 32];
pub type Version = LengthVec<u8, u8>;

#[orga]
#[derive(Debug, Clone)]
pub struct Signal {
    pub version: Version,
    pub time: i64,
}

#[orga(skip(Default), version = 1)]
pub struct Upgrade {
    pub signals: Map<PubKey, Signal>,
    pub threshold: Decimal,
    pub activation_delay_seconds: i64,
    pub rate_limit_seconds: i64,
    #[state(absolute_prefix(b"/version"))]
    // TODO: use Value/Box instead of Map<(), _>
    pub current_version: Map<(), Version>,
}

impl Default for Upgrade {
    fn default() -> Self {
        let mut current_version = Map::new();
        current_version
            .insert((), vec![0].try_into().unwrap())
            .unwrap();
        Self {
            signals: Default::default(),
            threshold: (Amount::new(2) / Amount::new(3)).result().unwrap(),
            activation_delay_seconds: 60 * 60 * 24,
            rate_limit_seconds: 60,
            current_version,
        }
    }
}

impl MigrateFrom<UpgradeV0> for UpgradeV1 {
    fn migrate_from(_prev: UpgradeV0) -> Result<Self> {
        unreachable!()
    }
}

#[orga]
impl Upgrade {
    #[call]
    pub fn signal(&mut self, version: Version) -> Result<()> {
        crate::plugins::disable_fee();
        let cons_key = self.signer_cons_key()?;
        let now = self.current_seconds()?;

        let signal = Signal { version, time: now };
        if let Some(prev_signal) = self.signals.get(cons_key)? {
            let soonest = prev_signal.time + self.rate_limit_seconds;
            if signal.time < soonest {
                return Err(OrgaError::App(format!(
                    "Must wait {} seconds before signaling again",
                    soonest - signal.time
                )));
            }
            if signal.version == prev_signal.version {
                return Err(OrgaError::App(format!(
                    "Version {:?} has already been signaled",
                    signal.version
                )));
            }
        }

        self.signals.insert(cons_key, signal)
    }

    pub fn step(&mut self, bin_version: &Version, upgrade_authorized: bool) -> Result<()> {
        let bin_version = bin_version.clone();
        let net_version = self.current_version.get(())?.unwrap().clone();
        if bin_version != net_version {
            return Err(Error::Version {
                expected: net_version,
                actual: bin_version,
            }
            .into());
        }

        if !upgrade_authorized {
            return Ok(());
        }
        if let Some(new_version) = self.upgrade_ready()? {
            self.current_version.insert((), new_version)?;
        }

        Ok(())
    }

    fn upgrade_ready(&mut self) -> Result<Option<Version>> {
        let now = self.current_seconds()?;
        let latest_counted_time = now - self.activation_delay_seconds;
        let mut total_vp = 0;
        let mut signal_vps = HashMap::new();
        for validator in self.current_validators()? {
            total_vp += validator.power;
            if let Some(signal) = self.signals.get(validator.pubkey)? {
                if signal.time <= latest_counted_time
                    // TODO: implement comparison between LengthVec and Vec
                    && signal.version.clone()
                        != *self.current_version.get(())?.unwrap()
                    && validator.power > 0
                {
                    *signal_vps.entry(signal.version.clone()).or_default() += validator.power;
                }
            }
        }
        let vp_threshold = (self.threshold * Amount::new(total_vp))?;

        Ok(signal_vps
            .into_iter()
            .find(|(_, vp)| Amount::new(*vp) > vp_threshold)
            .map(|(version, _)| version))
    }

    fn current_seconds(&mut self) -> Result<i64> {
        let time = self
            .context::<Time>()
            .ok_or_else(|| OrgaError::Coins("No Time context available".into()))?;

        Ok(time.seconds)
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| OrgaError::Coins("No Signer context available".into()))?
            .signer
            .ok_or_else(|| OrgaError::Coins("Call must be signed".into()))
    }

    fn signer_cons_key(&mut self) -> Result<PubKey> {
        let signer = self.signer()?;
        let validators: &mut Validators = self
            .context()
            .ok_or_else(|| OrgaError::App("No validator context found".to_string()))?;

        validators
            .consensus_key(signer)?
            .ok_or_else(|| OrgaError::App("Signer does not have a consensus key".to_string()))
    }

    fn current_validators(&mut self) -> Result<Vec<ValidatorEntry>> {
        let validators: &mut Validators = self
            .context()
            .ok_or_else(|| OrgaError::App("No validator context found".to_string()))?;
        validators.entries()
    }
}

pub fn load_version(store: Store) -> Result<Option<Vec<u8>>> {
    let store = store.with_prefix(vec![]);
    store.get(VERSION_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use serial_test::serial;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn setup_validators() {
        let mut val_ctx = Validators::new(
            Rc::new(RefCell::new(Some(Default::default()))),
            Rc::new(RefCell::new(Some(Default::default()))),
        );
        let vals = vec![
            ([0; 32], [0; 20], 10),
            ([1; 32], [1; 20], 10),
            ([2; 32], [2; 20], 11),
        ];
        for (cons_key, op_key, vp) in vals {
            val_ctx.set_voting_power(cons_key, vp);
            val_ctx.set_operator(cons_key, op_key).unwrap();
        }

        Context::add(val_ctx);
    }

    fn set_time(t: i64) {
        Context::add(Time::from_seconds(t));
    }

    fn set_signer(op_key: [u8; 20]) {
        Context::add(Signer {
            signer: Some(op_key.into()),
        })
    }

    #[test]
    #[serial]
    fn upgrade_coordination() -> Result<()> {
        setup_validators();
        set_time(0);
        let version: Version = vec![0].try_into().unwrap();
        let next_version: Version = vec![1].try_into().unwrap();
        let mut upgrade = Upgrade {
            activation_delay_seconds: 10,
            rate_limit_seconds: 5,
            ..Default::default()
        };
        upgrade.current_version.insert((), version.clone())?;

        assert!(upgrade.upgrade_ready()?.is_none());
        upgrade.step(&version, true)?;
        assert_eq!(&*upgrade.current_version.get(())?.unwrap(), &version);
        set_signer([0; 20]);
        upgrade.signal(next_version.clone())?;
        set_time(1);
        assert!(upgrade.signal(next_version.clone()).is_err());
        set_signer([2; 20]);
        upgrade.signal(next_version.clone())?;
        assert!(upgrade.upgrade_ready()?.is_none());
        upgrade.step(&version, true)?;
        assert!(upgrade.step(&next_version, true).is_err());
        assert_eq!(&*upgrade.current_version.get(())?.unwrap(), &version);
        set_time(12);
        assert!(upgrade.upgrade_ready()?.unwrap() == next_version);
        assert_eq!(&*upgrade.current_version.get(())?.unwrap(), &version);
        upgrade.step(&version, false)?;
        assert_eq!(&*upgrade.current_version.get(())?.unwrap(), &version);
        upgrade.step(&version, true)?;
        assert_eq!(&*upgrade.current_version.get(())?.unwrap(), &next_version);
        assert!(upgrade.step(&version, true).is_err());
        upgrade.step(&next_version, true)?;

        Ok(())
    }
}
