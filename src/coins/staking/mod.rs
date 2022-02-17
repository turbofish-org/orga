use super::decimal::DecimalEncoding;
use super::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use super::{Address, Amount, Balance, Coin, Decimal, Give, Pool, Symbol};
#[cfg(feature = "abci")]
use crate::abci::{BeginBlock, EndBlock};
use crate::call::Call;
use crate::client::Client;
use crate::collections::{Deque, Entry, EntryMap, Map};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
#[cfg(feature = "abci")]
use crate::plugins::{BeginBlockCtx, EndBlockCtx, Time, Validators};
use crate::plugins::{Paid, Signer};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use rust_decimal_macros::dec;
use sha2::{Digest, Sha256};
use std::convert::TryInto;
use tendermint_proto::abci::EvidenceType;

mod delegator;
pub use delegator::*;

mod validator;
pub use validator::*;

#[cfg(test)]
const UNBONDING_SECONDS: u64 = 10; // 10 seconds
#[cfg(not(test))]
const UNBONDING_SECONDS: u64 = 60 * 60 * 24 * 7 * 2; // 2 weeks
const MAX_OFFLINE_BLOCKS: u64 = 50_000; // ~14 hours for 1s blocks
const MAX_VALIDATORS: u64 = 100;
const MIN_SELF_DELEGATION_MIN: u64 = 0;
const DOWNTIME_JAIL_SECONDS: u64 = 60 * 60 * 24; // 1 day
const EDIT_INTERVAL_SECONDS: u64 = 60 * 60 * 24; // 1 day

#[derive(Call, Query, Client)]
pub struct Staking<S: Symbol> {
    validators: Pool<Address, Validator<S>, S>,
    consensus_keys: Map<Address, [u8; 32]>,
    last_signed_block: Map<[u8; 20], u64>,
    max_validators: u64,
    min_self_delegation_min: u64,
    validators_by_power: EntryMap<ValidatorPowerEntry>,
    last_indexed_power: Map<Address, u64>,
    last_validator_powers: Map<Address, u64>,
    address_for_tm_hash: Map<[u8; 20], Address>,
    unbonding_seconds: u64,
    max_offline_blocks: u64,
    slash_fraction_double_sign: Decimal,
    slash_fraction_downtime: Decimal,
    downtime_jail_seconds: u64,
    validator_queue: EntryMap<ValidatorQueueEntry>,
    unbonding_delegation_queue: Deque<UnbondingDelegationEntry>,
}

#[derive(Entry, Clone)]
struct ValidatorQueueEntry {
    #[key]
    start_seconds: i64,
    #[key]
    address_bytes: [u8; 20],
}

impl EntryMap<ValidatorQueueEntry> {
    fn remove_by_address(&mut self, address: Address) -> Result<()> {
        let entries: Vec<Result<_>> = self.iter()?.collect();
        for res in entries {
            let entry = res?;
            if entry.address_bytes == address.bytes() {
                self.delete(ValidatorQueueEntry {
                    start_seconds: entry.start_seconds,
                    address_bytes: entry.address_bytes,
                })?;
            }
        }
        Ok(())
    }
}

#[derive(State)]
pub struct UnbondingDelegationEntry {
    validator_address: Address,
    delegator_address: Address,
    start_seconds: i64,
}

#[derive(Entry)]
struct ValidatorPowerEntry {
    #[key]
    inverted_power: u64,
    #[key]
    address_bytes: [u8; 20],
}

impl ValidatorPowerEntry {
    fn power(&self) -> u64 {
        u64::max_value() - self.inverted_power
    }
}

#[cfg(feature = "abci")]
impl<S: Symbol> EndBlock for Staking<S> {
    fn end_block(&mut self, _ctx: &EndBlockCtx) -> Result<()> {
        self.end_block_step()
    }
}

impl<S: Symbol> State for Staking<S> {
    type Encoding = StakingEncoding<S>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            validators: State::create(store.sub(&[0]), data.validators)?,
            min_self_delegation_min: State::create(store.sub(&[1]), data.min_self_delegation_min)?,
            consensus_keys: State::create(store.sub(&[2]), ())?,
            last_signed_block: State::create(store.sub(&[3]), ())?,
            validators_by_power: State::create(store.sub(&[4]), ())?,
            last_validator_powers: State::create(store.sub(&[5]), ())?,
            max_validators: State::create(store.sub(&[6]), data.max_validators)?,
            last_indexed_power: State::create(store.sub(&[7]), ())?,
            address_for_tm_hash: State::create(store.sub(&[8]), ())?,
            unbonding_seconds: State::create(store.sub(&[9]), data.unbonding_seconds)?,
            max_offline_blocks: State::create(store.sub(&[10]), data.max_offline_blocks)?,
            slash_fraction_double_sign: State::create(
                store.sub(&[11]),
                data.slash_fraction_double_sign,
            )?,
            slash_fraction_downtime: State::create(store.sub(&[12]), data.slash_fraction_downtime)?,
            downtime_jail_seconds: State::create(store.sub(&[13]), data.downtime_jail_seconds)?,
            validator_queue: State::create(store.sub(&[14]), ())?,
            unbonding_delegation_queue: State::create(
                store.sub(&[15]),
                data.unbonding_delegation_queue,
            )?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.consensus_keys.flush()?;
        self.last_signed_block.flush()?;
        self.last_validator_powers.flush()?;
        self.last_indexed_power.flush()?;
        self.validators_by_power.flush()?;
        self.address_for_tm_hash.flush()?;
        Ok(Self::Encoding {
            max_validators: self.max_validators,
            min_self_delegation_min: self.min_self_delegation_min,
            validators: self.validators.flush()?,
            unbonding_seconds: self.unbonding_seconds,
            max_offline_blocks: self.max_offline_blocks,
            slash_fraction_double_sign: self.slash_fraction_double_sign.into(),
            slash_fraction_downtime: self.slash_fraction_downtime.into(),
            downtime_jail_seconds: self.downtime_jail_seconds,
            unbonding_delegation_queue: self.unbonding_delegation_queue.flush()?,
        })
    }
}

impl<S: Symbol> From<Staking<S>> for StakingEncoding<S> {
    fn from(staking: Staking<S>) -> Self {
        Self {
            max_validators: staking.max_validators,
            min_self_delegation_min: staking.min_self_delegation_min,
            unbonding_seconds: staking.unbonding_seconds,
            max_offline_blocks: staking.max_offline_blocks,
            slash_fraction_double_sign: staking.slash_fraction_double_sign.into(),
            slash_fraction_downtime: staking.slash_fraction_downtime.into(),
            downtime_jail_seconds: staking.downtime_jail_seconds,
            validators: staking.validators.into(),
            unbonding_delegation_queue: staking.unbonding_delegation_queue.into(),
        }
    }
}

#[cfg(feature = "abci")]
impl<S: Symbol> BeginBlock for Staking<S> {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        if let Some(last_commit_info) = &ctx.last_commit_info {
            let height = ctx.height;
            // Update last online height
            last_commit_info
                .votes
                .iter()
                .filter(|vote_info| vote_info.signed_last_block)
                .filter(|vote_info| vote_info.validator.is_some())
                .map(|vote_info| vote_info.validator.as_ref().unwrap())
                .try_for_each(|validator| {
                    self.last_signed_block.insert(
                        validator.address[..].try_into().map_err(|_| {
                            Error::Coins("Invalid pubkey length from Tendermint".into())
                        })?,
                        height,
                    )
                })?;

            let mut offline_validator_hashes: Vec<[u8; 20]> = vec![];
            self.last_signed_block
                .iter()?
                .try_for_each(|res| -> Result<()> {
                    let (hash, last_height) = res?;
                    if *last_height + MAX_OFFLINE_BLOCKS < height {
                        offline_validator_hashes.push(hash.to_vec().try_into().map_err(|_| {
                            Error::Coins("Invalid pub key hash length from Tendermint".into())
                        })?);
                    }

                    Ok(())
                })?;

            for hash in offline_validator_hashes.iter() {
                if let Some(address) = self.address_for_tm_hash.get(*hash)? {
                    let address = *address;
                    let validator = self.validators.get(address)?;
                    let in_active_set = validator.in_active_set;
                    drop(validator);
                    if in_active_set {
                        self.punish_downtime(address)?;
                    }
                    self.last_signed_block.remove(*hash)?;
                }
            }
        }

        for evidence in &ctx.byzantine_validators {
            match &evidence.validator {
                Some(validator) => {
                    let hash: [u8; 20] = validator.address.clone().try_into().map_err(|_| {
                        Error::Coins("Invalid pubkey length from Tendermint".into())
                    })?;
                    match self.address_for_tm_hash.get(hash)? {
                        Some(address) => {
                            let address = *address;
                            match evidence.r#type() {
                                EvidenceType::DuplicateVote => {
                                    self.punish_double_sign(address)?;
                                }
                                EvidenceType::LightClientAttack => {
                                    self.punish_light_client_attack(address)?;
                                }
                                _ => {}
                            };
                        }
                        None => {
                            return Err(Error::Coins(
                                "Invalid pubkey length from Tendermint".into(),
                            ));
                        }
                    }
                }
                None => {}
            }
        }

        Ok(())
    }
}

#[derive(Encode, Decode)]
pub struct StakingEncoding<S: Symbol> {
    max_validators: u64,
    min_self_delegation_min: u64,
    unbonding_seconds: u64,
    max_offline_blocks: u64,
    slash_fraction_double_sign: DecimalEncoding,
    slash_fraction_downtime: DecimalEncoding,
    downtime_jail_seconds: u64,
    validators: <Pool<Address, Validator<S>, S> as State>::Encoding,
    unbonding_delegation_queue: <Deque<UnbondingDelegationEntry> as State>::Encoding,
}

impl<S: Symbol> Default for StakingEncoding<S> {
    fn default() -> Self {
        let slash_fraction_double_sign: Decimal = dec!(0.05).into();
        let slash_fraction_downtime: Decimal = dec!(0.01).into();
        Self {
            max_validators: MAX_VALIDATORS,
            min_self_delegation_min: MIN_SELF_DELEGATION_MIN,
            unbonding_seconds: UNBONDING_SECONDS,
            max_offline_blocks: MAX_OFFLINE_BLOCKS,
            slash_fraction_double_sign: slash_fraction_double_sign.into(),
            slash_fraction_downtime: slash_fraction_downtime.into(),
            downtime_jail_seconds: DOWNTIME_JAIL_SECONDS,
            validators: Default::default(),
            unbonding_delegation_queue: Default::default(),
        }
    }
}

impl<S: Symbol> Staking<S> {
    pub fn delegate(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        coins: Coin<S>,
    ) -> Result<()> {
        let _ = self.consensus_key(val_address)?;
        {
            let mut validator = self.validators.get_mut(val_address)?;
            let mut delegator = validator.get_mut(delegator_address)?;
            delegator.add_stake(coins)?;
            if val_address == delegator_address {
                drop(delegator);
                validator.update_self_delegation()?;
            }
        }

        self.update_vp(val_address)
    }

    fn consensus_key(&self, val_address: Address) -> Result<[u8; 32]> {
        let consensus_key = match self.consensus_keys.get(val_address)? {
            Some(key) => *key,
            None => return Err(Error::Coins("Validator is not declared".into())),
        };

        Ok(consensus_key)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn declare(
        &mut self,
        val_address: Address,
        consensus_key: [u8; 32],
        commission: Decimal,
        commission_max: Decimal,
        commission_max_change: Decimal,
        min_self_delegation: Amount,
        validator_info: ValidatorInfo,
        coins: Coin<S>,
    ) -> Result<()> {
        let declared = self.consensus_keys.contains_key(val_address)?;
        if declared {
            return Err(Error::Coins("Validator is already declared".into()));
        }
        if coins.amount < min_self_delegation {
            return Err(Error::Coins("Insufficient self-delegation".into()));
        }
        let tm_hash = tm_pubkey_hash(consensus_key)?;
        let tm_hash_exists = self.address_for_tm_hash.contains_key(tm_hash)?;
        if tm_hash_exists {
            return Err(Error::Coins(
                "Tendermint public key is already in use".into(),
            ));
        }

        if commission < Decimal::zero() || commission > commission_max {
            return Err(Error::Coins(
                "Initial commission must be between 0 and max commission".into(),
            ));
        }
        if commission_max < Decimal::zero() || commission_max > Decimal::one() {
            return Err(Error::Coins(
                "Max commission must be between 0 and 1".into(),
            ));
        }
        if commission_max_change < Decimal::zero() || commission_max_change > commission_max {
            return Err(Error::Coins(
                "Max commission change must be between 0 and max commission".into(),
            ));
        }
        if min_self_delegation < self.min_self_delegation_min {
            return Err(Error::Coins(
                "Min self-delegation setting is too small".into(),
            ));
        }

        self.consensus_keys
            .insert(val_address, consensus_key.into())?;

        self.address_for_tm_hash
            .insert(tm_hash, val_address.into())?;

        let mut validator = self.validators.get_mut(val_address)?;
        validator.commission.rate = commission;
        validator.commission.max = commission_max;
        validator.commission.max_change = commission_max_change;
        validator.min_self_delegation = min_self_delegation;
        validator.address = val_address;
        validator.info = validator_info;
        validator.last_edited_seconds = i32::MIN as i64;
        drop(validator);

        self.delegate(val_address, val_address, coins)
    }

    pub fn edit_validator(
        &mut self,
        val_address: Address,
        commission: Decimal,
        min_self_delegation: Amount,
        validator_info: ValidatorInfo,
    ) -> Result<()> {
        let now = self.current_seconds()?;
        let mut validator = self.validators.get_mut(val_address)?;

        if validator.self_delegation < min_self_delegation {
            return Err(Error::Coins(
                "Min self-delegation cannot exceed current staked amount".into(),
            ));
        }

        if min_self_delegation < validator.min_self_delegation {
            return Err(Error::Coins(
                "Min self-delegation setting may not decrease".into(),
            ));
        }

        if commission < Decimal::zero() || commission > validator.commission.max {
            return Err(Error::Coins(
                "Commission must be between 0 and max commission".into(),
            ));
        }
        let change = (commission - validator.commission.rate)?.abs();
        if change > validator.commission.max_change {
            return Err(Error::Coins(
                "Commission change is greater than the validator's commission max change setting"
                    .into(),
            ));
        }
        if now - (EDIT_INTERVAL_SECONDS as i64) < validator.last_edited_seconds {
            return Err(Error::Coins(
                "Validators may only be edited once per 24 hours".into(),
            ));
        }
        validator.commission.rate = commission;
        validator.info = validator_info;
        validator.min_self_delegation = min_self_delegation;

        validator.last_edited_seconds = now;

        Ok(())
    }

    pub fn staked(&self) -> Result<Amount> {
        self.validators.balance()?.amount()
    }

    pub fn set_min_self_delegation(&mut self, min_self_delegation: u64) {
        self.min_self_delegation_min = min_self_delegation;
    }

    pub fn set_max_validators(&mut self, max_validators: u64) {
        self.max_validators = max_validators;
    }

    fn punish_downtime(&mut self, val_address: Address) -> Result<()> {
        {
            let mut validator = self.validators.get_mut(val_address)?;
            validator.jail_for_seconds(self.downtime_jail_seconds)?;
            validator.slash(self.slash_fraction_downtime)?;
        }
        self.update_vp(val_address)
    }

    fn punish_double_sign(&mut self, val_address: Address) -> Result<()> {
        {
            let mut validator = self.validators.get_mut(val_address)?;
            validator.jail_forever();
            validator.slash(self.slash_fraction_double_sign)?;
        }
        self.update_vp(val_address)
    }

    fn punish_light_client_attack(&mut self, val_address: Address) -> Result<()> {
        // Currently the same punishment as double sign evidence
        self.punish_double_sign(val_address)
    }

    pub fn withdraw<A: Into<Amount>>(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        amount: A,
    ) -> Result<Coin<S>> {
        let amount = amount.into();
        let mut validator = self.validators.get_mut(val_address)?;
        let mut delegator = validator.get_mut(delegator_address)?;
        delegator.process_unbonds()?;

        delegator.withdraw_liquid(amount)
    }

    pub fn unbond<A: Into<Amount>>(
        &mut self,
        validator_address: Address,
        delegator_address: Address,
        amount: A,
    ) -> Result<()> {
        let start_seconds = {
            let amount = amount.into();
            let now = self.current_seconds()?;
            let mut validator = self.validators.get_mut(validator_address)?;
            let status = validator.status();
            let start_seconds = match status {
                Status::Unbonding { start_seconds } => Some(start_seconds),
                Status::Bonded => Some(now),
                Status::Unbonded => None,
            };
            let mut delegator = validator.get_mut(delegator_address)?;

            delegator.unbond(amount, start_seconds)?;
            if validator_address == delegator_address {
                drop(delegator);
                validator.update_self_delegation()?;
            }
            start_seconds
        };

        if let Some(start_seconds) = start_seconds {
            self.unbonding_delegation_queue.push_back(
                UnbondingDelegationEntry {
                    delegator_address,
                    validator_address,
                    start_seconds,
                }
                .into(),
            )?;
        }

        self.update_vp(validator_address)
    }

    pub fn get(&self, val_address: Address) -> Result<PoolChild<Validator<S>, S>> {
        self.validators.get(val_address)
    }

    pub fn get_mut(
        &mut self,
        val_address: Address,
    ) -> Result<PoolChildMut<Address, Validator<S>, S>> {
        self.validators.get_mut(val_address)
    }

    #[query]
    pub fn delegations(
        &self,
        delegator_address: Address,
    ) -> Result<Vec<(Address, DelegationInfo)>> {
        self.validators
            .iter()?
            .map(|entry| {
                let (val_address, validator) = entry?;
                let delegator = validator.get(delegator_address)?;

                Ok((val_address, delegator.info()?))
            })
            .collect()
    }

    #[query]
    pub fn all_validators(&self) -> Result<Vec<ValidatorQueryInfo>> {
        self.validators
            .iter()?
            .map(|entry| {
                let (_, validator) = entry?;
                let info = validator.query_info()?;

                Ok(info)
            })
            .collect()
    }

    #[call]
    pub fn unbond_self(&mut self, val_address: Address, amount: Amount) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        self.unbond(val_address, signer, amount)
    }

    #[call]
    #[allow(clippy::too_many_arguments)]
    pub fn declare_self(
        &mut self,
        consensus_key: [u8; 32],
        commission: Decimal,
        commission_max: Decimal,
        commission_max_change: Decimal,
        min_self_delegation: Amount,
        amount: Amount,
        validator_info: ValidatorInfo,
    ) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        let payment = self.paid()?.take(amount)?;
        self.declare(
            signer,
            consensus_key,
            commission,
            commission_max,
            commission_max_change,
            min_self_delegation,
            validator_info,
            payment,
        )
    }

    #[call]
    pub fn delegate_from_self(&mut self, validator_address: Address, amount: Amount) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        let payment = self.paid()?.take(amount)?;
        self.delegate(validator_address, signer, payment)
    }

    #[call]
    pub fn take_as_funding(&mut self, validator_address: Address, amount: Amount) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        let taken_coins = self.withdraw(validator_address, signer, amount)?;
        self.paid()?.give::<S, _>(taken_coins.amount)
    }

    #[call]
    pub fn claim_all(&mut self) -> Result<()> {
        let signer = self.signer()?;
        let delegations = self.delegations(signer)?;
        delegations
            .iter()
            .try_for_each(|(val_address, delegation)| {
                if delegation.liquid > 0 {
                    self.take_as_funding(*val_address, delegation.liquid)
                } else {
                    Ok(())
                }
            })?;

        Ok(())
    }

    #[call]
    pub fn unjail(&mut self) -> Result<()> {
        let signer = self.signer()?;
        {
            let mut validator = self.validators.get_mut(signer)?;
            validator.try_unjail()?;
        }

        self.update_vp(signer)
    }

    #[call]
    pub fn edit_validator_self(
        &mut self,
        commission: Decimal,
        min_self_delegation: Amount,
        validator_info: ValidatorInfo,
    ) -> Result<()> {
        let val_address = self.signer()?;
        let _ = self.consensus_key(val_address)?;

        self.edit_validator(val_address, commission, min_self_delegation, validator_info)
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Coins("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Call must be signed".into()))
    }

    fn paid(&mut self) -> Result<&mut Paid> {
        self.context::<Paid>()
            .ok_or_else(|| Error::Coins("No Payment context available".into()))
    }

    fn update_vp(&mut self, val_address: Address) -> Result<()> {
        let mut validator = self.validators.get_mut(val_address)?;

        if validator.jailed {
            drop(validator);
            self.set_potential_voting_power(val_address, 0)
        } else {
            let vp = validator.staked()?.into();
            drop(validator);

            self.set_potential_voting_power(val_address, vp)
        }
    }

    fn set_potential_voting_power(&mut self, address: Address, power: u64) -> Result<()> {
        if let Some(last_indexed) = self.last_indexed_power.get(address)? {
            self.validators_by_power.delete(ValidatorPowerEntry {
                address_bytes: address.bytes(),
                inverted_power: u64::MAX - *last_indexed,
            })?;
        }

        self.validators_by_power.insert(ValidatorPowerEntry {
            address_bytes: address.bytes(),
            inverted_power: u64::MAX - power,
        })?;

        self.last_indexed_power.insert(address, power)
    }

    fn process_all_queues(&mut self) -> Result<()> {
        self.process_validator_queue()?;
        self.process_unbonding_delegation_queue()?;
        self.process_redelegation_queue()
    }

    fn process_validator_queue(&mut self) -> Result<()> {
        let now = self.current_seconds()?;
        let mut entries_to_unbond: Vec<ValidatorQueueEntry> = vec![];
        for entry in self.validator_queue.iter()? {
            let entry = entry?;
            let matured = now - entry.start_seconds >= self.unbonding_seconds as i64;
            if matured {
                entries_to_unbond.push(entry.clone());
            } else {
                break;
            }
        }

        for entry in entries_to_unbond.into_iter() {
            self.transition_to_unbonded(entry.address_bytes.into())?;
            self.validator_queue.delete(entry)?;
        }

        Ok(())
    }

    fn process_unbonding_delegation_queue(&mut self) -> Result<()> {
        let now = self.current_seconds()?;

        while let Some(unbond) = self.unbonding_delegation_queue.front()? {
            let matured = now - unbond.start_seconds >= self.unbonding_seconds as i64;
            if matured {
                let unbond = self
                    .unbonding_delegation_queue
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Unbonding delegation queue is empty".into()))?;
                let mut validator = self.validators.get_mut(unbond.validator_address)?;
                let mut delegator = validator.get_mut(unbond.delegator_address)?;
                delegator.process_unbonds()?;
            } else {
                break;
            }
        }

        Ok(())
    }

    fn process_redelegation_queue(&mut self) -> Result<()> {
        Ok(())
    }

    #[cfg(feature = "abci")]
    fn end_block_step(&mut self) -> Result<()> {
        self.process_all_queues()?;
        use std::collections::HashSet;
        let max_vals = self.max_validators;
        let mut new_val_entries: Vec<(Address, u64)> = vec![];
        let mut i = 0;
        // Collect the top validators by voting power
        for entry in self.validators_by_power.iter()? {
            let entry = entry?;
            let address: Address = entry.address_bytes.into();
            let new_power = entry.power();

            if new_power == 0 {
                break;
            }

            new_val_entries.push((address, new_power));

            i += 1;
            if i == max_vals {
                break;
            }
        }

        // Find the minimal set of updates required to send back to Tendermint
        let mut new_power_updates = vec![];
        for (address, power) in new_val_entries.iter() {
            match self.last_validator_powers.get(*address)? {
                Some(prev_power) => {
                    if *power != *prev_power {
                        new_power_updates.push((*address, *power));
                    }
                }
                None => new_power_updates.push((*address, *power)),
            }
        }

        let validators_in_active_set: HashSet<_> = new_val_entries
            .iter()
            .map(|(address, _)| *address)
            .collect();

        // Check for validators bumped from the active validator set
        for entry in self.last_validator_powers.iter()? {
            let (address, _power) = entry?;
            if !validators_in_active_set.contains(&address) {
                new_power_updates.push((*address, 0));
            }
        }

        // Tell newly-updated validators whether they're in the active set for
        // proper fee accounting
        for (address, power) in new_power_updates.iter() {
            let mut validator = self.validators.get_mut(*address)?;
            let in_active_set_before = validator.in_active_set;
            validator.in_active_set = *power > 0;
            let in_active_set_now = validator.in_active_set;
            drop(validator);

            match (in_active_set_before, in_active_set_now) {
                (true, false) => {
                    self.transition_to_unbonding(*address)?;
                } // removed from active set
                (false, true) => {
                    self.transition_to_bonded(*address)?;
                } // added to active set
                _ => {}
            }
        }

        // Map to consensus key before we send back the updates
        let mut new_power_updates_con = vec![];
        for (address, power) in new_power_updates.iter() {
            let consensus_key = self
                .consensus_keys
                .get(*address)?
                .ok_or_else(|| Error::Coins("No consensus key for validator".into()))?;
            new_power_updates_con.push((*consensus_key, *power));
        }

        let val_ctx = self
            .context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?;

        for (consensus_key, power) in new_power_updates_con.into_iter() {
            val_ctx.set_voting_power(consensus_key, power);
        }

        // Update the last validator powers for use in the next block
        for (address, power) in new_power_updates.iter() {
            if *power > 0 {
                self.last_validator_powers.insert(*address, *power)?;
            } else {
                self.last_validator_powers.remove(*address)?;
            }
        }

        Ok(())
    }

    fn transition_to_bonded(&mut self, val_address: Address) -> Result<()> {
        let mut validator = self.validators.get_mut(val_address)?;
        validator.unbonding = false;
        self.validator_queue.remove_by_address(val_address)
    }

    fn transition_to_unbonding(&mut self, val_address: Address) -> Result<()> {
        let now = self.current_seconds()?;
        {
            let mut validator = self.validators.get_mut(val_address)?;
            validator.unbonding = true;
            validator.unbonding_start_seconds = now;
        }

        self.validator_queue.insert(ValidatorQueueEntry {
            start_seconds: now,
            address_bytes: val_address.bytes(),
        })
    }

    fn transition_to_unbonded(&mut self, val_address: Address) -> Result<()> {
        let mut validator = self.validators.get_mut(val_address)?;
        validator.unbonding = false;

        Ok(())
    }

    fn current_seconds(&mut self) -> Result<i64> {
        let time = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?;

        Ok(time.seconds)
    }
}

fn assert_positive(amount: Amount) -> Result<()> {
    if amount > 0 {
        Ok(())
    } else {
        Err(Error::Coins("Amount must be positive".into()))
    }
}

impl<S: Symbol> Give<S> for Staking<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        self.validators.give(coins)
    }
}

fn tm_pubkey_hash(consensus_key: [u8; 32]) -> Result<[u8; 20]> {
    let mut hasher = Sha256::new();
    hasher.update(consensus_key);
    let hash = hasher.finalize().to_vec()[..20].to_vec();

    hash.try_into()
        .map_err(|_| Error::Coins("Invalid consensus key".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "abci")]
    use crate::plugins::Time;
    use crate::{
        context::Context,
        store::{MapStore, Shared, Store},
    };
    use rust_decimal_macros::dec;
    use serial_test::serial;

    #[derive(State, Debug, Clone)]
    struct Simp(());
    impl Symbol for Simp {}

    #[cfg(feature = "abci")]
    #[test]
    #[serial]
    fn staking() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;
        staking.slash_fraction_downtime = (Amount::new(1) / Amount::new(2))?;

        let alice = Address::from_pubkey([0; 33]);
        let alice_con = [4; 32];
        let bob = Address::from_pubkey([1; 33]);
        let bob_con = [5; 32];
        let carol = Address::from_pubkey([2; 33]);
        let dave = Address::from_pubkey([3; 33]);
        let dave_con = [6; 32];

        Context::add(Validators::default());
        Context::add(Time::from_seconds(0));

        staking
            .give(100.into())
            .expect_err("Cannot give to empty validator set");
        assert_eq!(staking.staked()?, 0);
        staking
            .delegate(alice, alice, Coin::mint(100))
            .expect_err("Should not be able to delegate to an undeclared validator");
        staking.declare(
            alice,
            alice_con,
            dec!(0.0).into(),
            dec!(1.0).into(),
            dec!(0.1).into(),
            0.into(),
            vec![].into(),
            50.into(),
        )?;
        staking
            .declare(
                alice,
                alice_con,
                dec!(0.0).into(),
                dec!(1.0).into(),
                dec!(0.1).into(),
                0.into(),
                vec![].into(),
                50.into(),
            )
            .expect_err("Should not be able to redeclare validator");
        staking
            .declare(
                carol,
                alice_con,
                dec!(0.0).into(),
                dec!(1.0).into(),
                dec!(0.1).into(),
                0.into(),
                vec![].into(),
                50.into(),
            )
            .expect_err("Should not be able to declare using an existing consensus key");

        staking.end_block_step()?;
        assert_eq!(staking.staked()?, 50);
        staking.delegate(alice, alice, Coin::mint(50))?;
        assert_eq!(staking.staked()?, 100);
        staking.declare(
            bob,
            bob_con,
            dec!(0.0).into(),
            dec!(1.0).into(),
            dec!(0.1).into(),
            0.into(),
            vec![].into(),
            50.into(),
        )?;
        staking.end_block_step()?;
        assert_eq!(staking.staked()?, 150);

        staking.delegate(bob, bob, Coin::mint(250))?;
        staking.delegate(bob, carol, Coin::mint(100))?;
        staking.delegate(bob, carol, Coin::mint(200))?;
        staking.delegate(bob, dave, Coin::mint(400))?;
        assert_eq!(staking.staked()?, 1100);

        let ctx = Context::resolve::<Validators>().unwrap();
        staking.end_block_step()?;
        let alice_vp = ctx.updates.get(&alice_con).unwrap().power;
        assert_eq!(alice_vp, 100);

        let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
        assert_eq!(bob_vp, 1000);

        let alice_self_delegation = staking.get(alice)?.get(alice)?.staked.amount()?;
        assert_eq!(alice_self_delegation, 100);

        let bob_self_delegation = staking.get(bob)?.get(bob)?.staked.amount()?;
        assert_eq!(bob_self_delegation, 300);

        let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
        assert_eq!(carol_to_bob_delegation, 300);

        let alice_val_balance = staking.get_mut(alice)?.staked()?;
        assert_eq!(alice_val_balance, 100);

        let bob_val_balance = staking.get_mut(bob)?.staked()?;
        assert_eq!(bob_val_balance, 1000);

        // Big block rewards, doubling all balances
        staking.give(Coin::mint(600))?;
        staking.give(Coin::mint(500))?;
        assert_eq!(staking.staked()?, 1100);

        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 100);

        let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
        assert_eq!(carol_to_bob_delegation, 300);
        let carol_to_bob_liquid = staking.get(bob)?.get(carol)?.liquid.amount()?;
        assert_eq!(carol_to_bob_liquid, 300);

        let bob_val_balance = staking.get_mut(bob)?.staked()?;
        assert_eq!(bob_val_balance, 1000);

        let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
        assert_eq!(bob_vp, 1000);

        // Bob gets slashed 50%
        staking.punish_downtime(bob)?;

        staking.end_block_step()?;
        // Bob has been jailed and should no longer have any voting power
        let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
        assert_eq!(bob_vp, 0);

        staking
            .withdraw(bob, dave, 401)
            .expect_err("Dave has not unbonded coins yet");
        // Bob's staked coins should no longer be present in the global staking
        // balance
        assert_eq!(staking.staked()?, 100);

        // Carol can still withdraw her 300 coins from Bob's jailed validator
        {
            staking.unbond(bob, carol, 150)?;
            assert_eq!(staking.staked()?, 100);
            staking
                .withdraw(bob, carol, 450)
                .expect_err("Should not be able to take coins before unbonding period has elapsed");
            assert_eq!(staking.staked()?, 100);
            Context::add(Time::from_seconds(10));
            let carol_recovered_coins = staking.withdraw(bob, carol, 450)?;

            assert_eq!(carol_recovered_coins.amount, 450);
        }

        {
            // Bob withdraws a third of his self-delegation
            staking.unbond(bob, bob, 100)?;
            Context::add(Time::from_seconds(20));
            let bob_recovered_coins = staking.withdraw(bob, bob, 100)?;
            assert_eq!(bob_recovered_coins.amount, 100);
            staking
                .unbond(bob, bob, 201)
                .expect_err("Should not be able to unbond more than we have staked");

            staking.unbond(bob, bob, 50)?;
            Context::add(Time::from_seconds(30));
            staking
                .withdraw(bob, bob, 501)
                .expect_err("Should not be able to take more than we have unbonded");
            staking.withdraw(bob, bob, 350)?.burn();
        }

        assert_eq!(staking.staked()?, 100);
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 100);
        let alice_staked = staking.get(alice)?.get(alice)?.staked.amount()?;
        assert_eq!(alice_staked, 100);

        // More block reward, but bob's delegators are jailed and should not
        // earn from it
        staking.give(Coin::mint(200))?;
        assert_eq!(staking.staked()?, 100);
        let alice_val_balance = staking.get_mut(alice)?.staked()?;
        assert_eq!(alice_val_balance, 100);
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 300);

        staking
            .unbond(bob, dave, 401)
            .expect_err("Dave should only have 400 unbondable coins");

        staking.unbond(bob, dave, 200)?;
        // Bob slashed another 50% while Dave unbonds
        staking.punish_downtime(bob)?;
        // dbg!(&staking.get_mut(bob)?.get_mut(dave)?.liquid);

        Context::add(Time::from_seconds(40));
        staking
            .withdraw(bob, dave, 501)
            .expect_err("Dave cannot take so many coins");
        staking.withdraw(bob, dave, 500)?.burn();

        assert_eq!(staking.staked()?, 100);
        staking.declare(
            dave,
            dave_con,
            dec!(0.0).into(),
            dec!(1.0).into(),
            dec!(0.1).into(),
            0.into(),
            vec![].into(),
            300.into(),
        )?;
        staking.end_block_step()?;
        assert_eq!(staking.staked()?, 400);
        staking.end_block_step()?;
        assert_eq!(ctx.updates.get(&alice_con).unwrap().power, 100);
        assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 300);
        staking.delegate(dave, carol, 300.into())?;
        assert_eq!(staking.staked()?, 700);

        staking.end_block_step()?;
        assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 600);
        staking.unbond(dave, dave, 150)?;
        assert_eq!(staking.staked()?, 550);
        staking.end_block_step()?;
        assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 450);

        // Test commissions
        let edith = Address::from_pubkey([7; 33]);
        let edith_con = [201; 32];

        staking.declare(
            edith,
            edith_con,
            dec!(0.5).into(),
            dec!(1.0).into(),
            dec!(0.2).into(),
            0.into(),
            vec![].into(),
            550.into(),
        )?;

        staking.delegate(edith, carol, 550.into())?;

        staking.get_mut(edith)?.give(500.into())?;

        let edith_liquid = staking.get(edith)?.get(edith)?.liquid.amount()?;
        assert_eq!(edith_liquid, 375);
        let carol_liquid = staking.get(edith)?.get(carol)?.liquid.amount()?;
        assert_eq!(carol_liquid, 125);

        staking.punish_double_sign(dave)?;
        staking.end_block_step()?;
        assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 0);

        Ok(())
    }

    #[cfg(feature = "abci")]
    #[test]
    #[serial]
    fn val_size_limit() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;

        Context::add(Validators::default());
        Context::add(Time::from_seconds(0));
        let ctx = Context::resolve::<Validators>().unwrap();
        staking.max_validators = 2;

        for i in 1..10 {
            staking.declare(
                Address::from_pubkey([i; 33]),
                [i; 32],
                dec!(0.0).into(),
                dec!(1.0).into(),
                dec!(0.1).into(),
                0.into(),
                vec![].into(),
                Amount::new(i as u64 * 100).into(),
            )?;
        }
        staking.end_block_step()?;
        assert_eq!(staking.staked()?, 1700);
        assert!(ctx.updates.get(&[7; 32]).is_none());
        assert_eq!(ctx.updates.get(&[8; 32]).unwrap().power, 800);
        assert_eq!(ctx.updates.get(&[9; 32]).unwrap().power, 900);
        staking.give(3400.into())?;
        assert_eq!(
            staking
                .get(Address::from_pubkey([4; 33]))?
                .get(Address::from_pubkey([4; 33]))?
                .liquid
                .amount()?,
            0
        );
        assert_eq!(
            staking
                .get(Address::from_pubkey([8; 33]))?
                .get(Address::from_pubkey([8; 33]))?
                .liquid
                .amount()?,
            1600
        );
        assert_eq!(
            staking
                .get(Address::from_pubkey([9; 33]))?
                .get(Address::from_pubkey([9; 33]))?
                .liquid
                .amount()?,
            1800
        );

        staking.declare(
            Address::from_pubkey([10; 33]),
            [10; 32],
            dec!(0.0).into(),
            dec!(1.0).into(),
            dec!(0.1).into(),
            0.into(),
            vec![].into(),
            Amount::new(1000).into(),
        )?;

        staking.end_block_step()?;

        assert_eq!(ctx.updates.get(&[8; 32]).unwrap().power, 0);
        assert_eq!(ctx.updates.get(&[9; 32]).unwrap().power, 900);
        assert_eq!(ctx.updates.get(&[10; 32]).unwrap().power, 1000);
        staking.give(1900.into())?;

        assert_eq!(
            staking
                .get(Address::from_pubkey([8; 33]))?
                .get(Address::from_pubkey([8; 33]))?
                .liquid
                .amount()?,
            1600
        );
        assert_eq!(
            staking
                .get(Address::from_pubkey([9; 33]))?
                .get(Address::from_pubkey([9; 33]))?
                .liquid
                .amount()?,
            2700
        );
        assert_eq!(
            staking
                .get(Address::from_pubkey([10; 33]))?
                .get(Address::from_pubkey([10; 33]))?
                .liquid
                .amount()?,
            1000
        );

        Ok(())
    }
}
