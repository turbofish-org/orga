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
use crate::plugins::{BeginBlockCtx, EndBlockCtx, Validators};
use crate::plugins::{Paid, Signer, Time};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use rust_decimal_macros::dec;
use sha2::{Digest, Sha256};
use std::convert::TryInto;
#[cfg(feature = "abci")]
use tendermint_proto::abci::EvidenceType;

mod delegator;
pub use delegator::*;

mod validator;
pub use validator::*;

#[cfg(test)]
const UNBONDING_SECONDS: u64 = 10; // 10 seconds
#[cfg(not(test))]
const UNBONDING_SECONDS: u64 = 60 * 60 * 24 * 14; // 2 weeks
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
    pub max_validators: u64,
    pub min_self_delegation_min: u64,
    validators_by_power: EntryMap<ValidatorPowerEntry>,
    last_indexed_power: Map<Address, u64>,
    last_validator_powers: Map<Address, u64>,
    address_for_tm_hash: Map<[u8; 20], Address>,
    unbonding_seconds: u64,
    pub max_offline_blocks: u64,
    pub slash_fraction_double_sign: Decimal,
    pub slash_fraction_downtime: Decimal,
    pub downtime_jail_seconds: u64,
    validator_queue: EntryMap<ValidatorQueueEntry>,
    unbonding_delegation_queue: Deque<UnbondingDelegationEntry>,
    redelegation_queue: Deque<RedelegationEntry>,
    delegation_index: Map<Address, Map<Address, ()>>,
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

#[derive(State)]
pub struct RedelegationEntry {
    src_validator_address: Address,
    dst_validator_address: Address,
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
    fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
        self.end_block_step(ctx)
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
            redelegation_queue: State::create(store.sub(&[16]), data.redelegation_queue)?,
            delegation_index: State::create(store.sub(&[17]), ())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.consensus_keys.flush()?;
        self.last_signed_block.flush()?;
        self.last_validator_powers.flush()?;
        self.last_indexed_power.flush()?;
        self.validators_by_power.flush()?;
        self.address_for_tm_hash.flush()?;
        self.validator_queue.flush()?;
        self.delegation_index.flush()?;
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
            redelegation_queue: self.redelegation_queue.flush()?,
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
            redelegation_queue: staking.redelegation_queue.into(),
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
                    if *last_height + self.max_offline_blocks < height {
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
    redelegation_queue: <Deque<RedelegationEntry> as State>::Encoding,
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
            redelegation_queue: Default::default(),
        }
    }
}

impl<S: Symbol> Staking<S> {
    pub fn validators(&self) -> &Pool<Address, Validator<S>, S> {
        &self.validators
    }

    pub fn consensus_keys(&self) -> &Map<Address, [u8; 32]> {
        &self.consensus_keys
    }

    pub fn delegate(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        coins: Coin<S>,
    ) -> Result<()> {
        let _ = self.consensus_key(val_address)?;
        {
            let mut validator = self.validators.get_mut(val_address)?;
            if validator.tombstoned {
                return Err(Error::Coins(
                    "Cannot delegate to a tombstoned validator".into(),
                ));
            }
            let mut delegator = validator.get_mut(delegator_address)?;
            delegator.add_stake(coins)?;
        }
        self.index_delegation(val_address, delegator_address)?;
        self.update_vp(val_address)
    }

    fn index_delegation(&mut self, val_address: Address, delegator_address: Address) -> Result<()> {
        self.delegation_index
            .entry(delegator_address)?
            .or_insert_default()?
            .insert(val_address, ())
    }

    fn consensus_key(&self, val_address: Address) -> Result<[u8; 32]> {
        let consensus_key = match self.consensus_keys.get(val_address)? {
            Some(key) => *key,
            None => return Err(Error::Coins("Validator is not declared".into())),
        };

        Ok(consensus_key)
    }

    pub fn declare(
        &mut self,
        val_address: Address,
        declaration: Declaration,
        coins: Coin<S>,
    ) -> Result<()> {
        let Declaration {
            min_self_delegation,
            consensus_key,
            commission,
            validator_info,
            ..
        } = declaration;
        let declared = self.consensus_keys.contains_key(val_address)?;
        if declared {
            return Err(Error::Coins("Validator is already declared".into()));
        }
        if coins.amount < min_self_delegation {
            return Err(Error::Coins("Insufficient self-delegation".into()));
        }
        validate_info(&validator_info)?;

        let tm_hash = tm_pubkey_hash(consensus_key)?;
        let tm_hash_exists = self.address_for_tm_hash.contains_key(tm_hash)?;
        if tm_hash_exists {
            return Err(Error::Coins(
                "Tendermint public key is already in use".into(),
            ));
        }

        if commission.rate < Decimal::zero() || commission.rate > commission.max {
            return Err(Error::Coins(
                "Initial commission must be between 0 and max commission".into(),
            ));
        }
        if commission.max < Decimal::zero() || commission.max > Decimal::one() {
            return Err(Error::Coins(
                "Max commission must be between 0 and 1".into(),
            ));
        }
        if commission.max_change < Decimal::zero() || commission.max_change > commission.max {
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

        #[cfg(feature = "abci")]
        let val_ctx = self
            .context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?;

        #[cfg(feature = "abci")]
        val_ctx.set_operator(consensus_key, val_address)?;

        let mut validator = self.validators.get_mut(val_address)?;
        validator.commission = commission;
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

        if validator.self_delegation()? < min_self_delegation {
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

        validate_info(&validator_info)?;

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

    fn punish_downtime(&mut self, val_address: Address) -> Result<()> {
        {
            let mut validator = self.validators.get_mut(val_address)?;
            validator.jail_for_seconds(self.downtime_jail_seconds)?;
            validator.slash(self.slash_fraction_downtime, true)?;
        }
        self.update_vp(val_address)
    }

    fn punish_double_sign(&mut self, val_address: Address) -> Result<()> {
        let redelegations = {
            let mut validator = self.validators.get_mut(val_address)?;
            validator.jail_forever();
            validator.slash(self.slash_fraction_double_sign, false)?
        };
        let multiplier = (Decimal::one() - self.slash_fraction_double_sign)?;
        for entry in redelegations.iter() {
            let del_address = entry.delegator_address;
            for redelegation in entry.outbound_redelegations.iter() {
                let mut validator = self.validators.get_mut(redelegation.address)?;
                let mut delegator = validator.get_mut(del_address)?;
                delegator.slash_redelegation((multiplier * redelegation.amount)?.amount()?)?;
            }
        }
        self.update_vp(val_address)
    }

    fn punish_light_client_attack(&mut self, val_address: Address) -> Result<()> {
        // Currently the same punishment as double sign evidence
        self.punish_double_sign(val_address)
    }

    pub fn deduct<A: Into<Amount>>(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        amount: A,
        denom: u8,
    ) -> Result<()> {
        let amount = amount.into();
        let mut validator = self.validators.get_mut(val_address)?;
        let mut delegator = validator.get_mut(delegator_address)?;
        delegator.process_unbonds()?;

        delegator.deduct(amount, denom)?;

        Ok(())
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
            let start_seconds = match validator.status() {
                Status::Bonded => Some(now),
                Status::Unbonding { start_seconds } => Some(start_seconds),
                Status::Unbonded => None,
            };
            let mut delegator = validator.get_mut(delegator_address)?;

            delegator.unbond(amount, start_seconds)?;

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

    pub fn redelegate<A: Into<Amount>>(
        &mut self,
        src_validator_address: Address,
        dst_validator_address: Address,
        delegator_address: Address,
        amount: A,
    ) -> Result<()> {
        if src_validator_address == dst_validator_address {
            return Err(Error::Coins(
                "Cannot redelegate to the same validator".into(),
            ));
        }
        let amount = amount.into();
        let now = self.current_seconds()?;

        let (coins, start_seconds) = {
            let mut src_validator = self.validators.get_mut(src_validator_address)?;
            let start_seconds = match src_validator.status() {
                Status::Bonded => Some(now),
                Status::Unbonding { start_seconds } => Some(start_seconds),
                Status::Unbonded => None,
            };
            let mut src_delegator = src_validator.get_mut(delegator_address)?;
            (
                src_delegator.redelegate_out(dst_validator_address, amount, start_seconds)?,
                start_seconds,
            )
        };

        {
            let _ = self.consensus_key(dst_validator_address)?;
            let mut dst_validator = self.validators.get_mut(dst_validator_address)?;
            if dst_validator.tombstoned {
                return Err(Error::Coins(
                    "Cannot redelegate to a tombstoned validator".into(),
                ));
            }
            if matches!(
                dst_validator.status(),
                Status::Unbonded | Status::Unbonding { .. }
            ) {
                return Err(Error::Coins(
                    "Cannot redelegate to an unbonding or unbonded validator".into(),
                ));
            }

            let mut dst_delegator = dst_validator.get_mut(delegator_address)?;
            dst_delegator.redelegate_in(src_validator_address, coins, start_seconds)?;
        }

        if let Some(start_seconds) = start_seconds {
            self.redelegation_queue.push_back(
                RedelegationEntry {
                    src_validator_address,
                    dst_validator_address,
                    delegator_address,
                    start_seconds,
                }
                .into(),
            )?;
        }

        self.index_delegation(dst_validator_address, delegator_address)?;
        self.update_vp(src_validator_address)?;
        self.update_vp(dst_validator_address)
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
        self.delegation_index
            .get_or_default(delegator_address)?
            .iter()?
            .map(|entry| {
                let (val_address, _) = entry?;
                let validator = self.validators.get(*val_address)?;
                let delegator = validator.get(delegator_address)?;

                Ok((*val_address, delegator.info()?))
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
    pub fn redelegate_self(
        &mut self,
        src_val_address: Address,
        dst_val_address: Address,
        amount: Amount,
    ) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        self.redelegate(src_val_address, dst_val_address, signer, amount)
    }

    #[call]
    pub fn declare_self(&mut self, declaration: Declaration) -> Result<()> {
        assert_positive(declaration.amount)?;
        let signer = self.signer()?;
        let payment = self.paid()?.take(declaration.amount)?;
        self.declare(signer, declaration, payment)
    }

    #[call]
    pub fn delegate_from_self(&mut self, validator_address: Address, amount: Amount) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        let payment = self.paid()?.take(amount)?;
        self.delegate(validator_address, signer, payment)
    }

    #[call]
    pub fn take_as_funding(
        &mut self,
        validator_address: Address,
        amount: Amount,
        denom: u8,
    ) -> Result<()> {
        assert_positive(amount)?;
        let signer = self.signer()?;
        self.deduct(validator_address, signer, amount, denom)?;
        self.paid()?.give_denom(amount, denom)
    }

    #[call]
    pub fn claim_all(&mut self) -> Result<()> {
        let signer = self.signer()?;
        let delegations = self.delegations(signer)?;
        delegations
            .iter()
            .try_for_each(|(val_address, delegation)| {
                for (denom, amount) in delegation.liquid.iter() {
                    if *amount > 0 {
                        self.take_as_funding(*val_address, *amount, *denom)?;
                    }
                }
                Ok::<_, Error>(())
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
        let vp = validator.potential_vp()?.into();
        drop(validator);
        self.set_potential_voting_power(val_address, vp)
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
        // TODO: should be one pass (needs drain iterator)
        self.validator_queue
            .iter()?
            .take_while(|entry| match entry {
                Ok(entry) => now - entry.start_seconds >= self.unbonding_seconds as i64,
                Err(_) => true,
            })
            .collect::<Vec<_>>()
            .into_iter()
            .try_for_each(|entry| {
                let entry = entry?;
                self.transition_to_unbonded(entry.address_bytes.into())?;
                self.validator_queue.delete(entry.clone())
            })
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
        let now = self.current_seconds()?;

        while let Some(redelegation) = self.redelegation_queue.front()? {
            let matured = now - redelegation.start_seconds >= self.unbonding_seconds as i64;
            if matured {
                let redelegation = self
                    .redelegation_queue
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Redelegation queue is empty".into()))?;

                {
                    let mut src_validator = self
                        .validators
                        .get_mut(redelegation.src_validator_address)?;
                    let mut src_delegator =
                        src_validator.get_mut(redelegation.delegator_address)?;
                    src_delegator.process_redelegations_out()?;
                }

                {
                    let mut dst_validator = self
                        .validators
                        .get_mut(redelegation.dst_validator_address)?;
                    let mut dst_delegator =
                        dst_validator.get_mut(redelegation.delegator_address)?;
                    dst_delegator.process_redelegations_in()?;
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    #[cfg(feature = "abci")]
    fn end_block_step(&mut self, ctx: &EndBlockCtx) -> Result<()> {
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
                    let tm_hash = tm_pubkey_hash(self.consensus_key(*address)?)?;
                    self.transition_to_unbonding(*address)?;
                    self.last_signed_block.remove(tm_hash)?;
                } // removed from active set
                (false, true) => {
                    let tm_hash = tm_pubkey_hash(self.consensus_key(*address)?)?;
                    self.transition_to_bonded(*address)?;
                    self.last_signed_block.insert(tm_hash, ctx.height)?;
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

fn validate_info(info: &ValidatorInfo) -> Result<()> {
    if info.bytes.len() > 5000 {
        return Err(Error::Coins("Validator info too long".into()));
    }

    Ok(())
}

impl<S: Symbol, T: Symbol> Give<Coin<T>> for Staking<S> {
    fn give(&mut self, coins: Coin<T>) -> Result<()> {
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

#[derive(Encode, Decode)]
pub struct Declaration {
    pub consensus_key: [u8; 32],
    pub commission: Commission,
    pub min_self_delegation: Amount,
    pub amount: Amount,
    pub validator_info: ValidatorInfo,
}

#[derive(State, Default, Encode, Decode, Clone, Copy)]
pub struct Commission {
    pub rate: Decimal,
    pub max: Decimal,
    pub max_change: Decimal,
}

#[cfg(test)]
mod tests;
