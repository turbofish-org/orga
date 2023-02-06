use crate::coins::{Address, Amount, Decimal};
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::LengthVec;
use crate::orga;
use crate::plugins::{Signer, Time, ValidatorEntry, Validators};
use crate::{Error as OrgaError, Result};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "Node is running version {expected:?}, but the network has upgraded to version {actual:?}"
    )]
    Version { expected: Version, actual: Version },
}

type PubKey = [u8; 32];
type Version = LengthVec<u8, u8>;

#[orga]
pub struct Signal {
    pub version: Version,
    pub time: i64,
}

#[orga(skip(Default))]
pub struct Upgrade {
    pub signals: Map<PubKey, Signal>,
    pub threshold: Decimal,
    pub activation_delay_seconds: i64,
    pub rate_limit_seconds: i64,
    pub current_version: Version,
}

impl Default for Upgrade {
    fn default() -> Self {
        Self {
            signals: Default::default(),
            threshold: (Amount::new(2) / Amount::new(3)).result().unwrap(),
            activation_delay_seconds: 60 * 60 * 24,
            rate_limit_seconds: 60,
            current_version: vec![0].try_into().unwrap(),
        }
    }
}

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

    pub fn step(&mut self, version: &Version) -> Result<()> {
        let version = version.clone();
        if self.current_version != version {
            return Err(Error::Version {
                expected: self.current_version.clone(),
                actual: version,
            }
            .into());
        }
        if let Some(new_version) = self.upgrade_ready()? {
            self.current_version = new_version;
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
                    && signal.version != self.current_version
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
            current_version: version.clone(),
            ..Default::default()
        };

        assert!(upgrade.upgrade_ready()?.is_none());
        upgrade.step(&version)?;
        assert_eq!(upgrade.current_version, version);
        set_signer([0; 20]);
        upgrade.signal(next_version.clone())?;
        set_time(1);
        assert!(upgrade.signal(next_version.clone()).is_err());
        set_signer([2; 20]);
        upgrade.signal(next_version.clone())?;
        assert!(upgrade.upgrade_ready()?.is_none());
        upgrade.step(&version)?;
        assert!(upgrade.step(&next_version).is_err());
        assert_eq!(upgrade.current_version, version);
        set_time(12);
        assert!(upgrade.upgrade_ready()?.unwrap() == next_version);
        assert_eq!(upgrade.current_version, version);
        upgrade.step(&version)?;
        assert_eq!(upgrade.current_version, next_version);
        assert!(upgrade.step(&version).is_err());
        upgrade.step(&next_version)?;

        Ok(())
    }
}
