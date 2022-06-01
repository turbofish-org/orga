use crate::call::Call;
use crate::client::Client;
use crate::coins::{Address, Amount, Coin, Give, Symbol, Take};
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
#[cfg(feature = "abci")]
use crate::migrate::Migrate;
use crate::plugins::Paid;
use crate::plugins::Signer;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};

#[derive(State, Encode, Decode, Call, Query, Client)]
pub struct Accounts<S: Symbol> {
    transfers_allowed: bool,
    transfer_exceptions: Map<Address, ()>,
    accounts: Map<Address, Coin<S>>,
}

impl<S: Symbol> Accounts<S> {
    #[call]
    pub fn transfer(&mut self, to: Address, amount: Amount) -> Result<()> {
        let signer = self.signer()?;
        if !self.transfers_allowed && !self.transfer_exceptions.contains_key(signer)? {
            return Err(Error::Coins("Transfers are currently disabled".into()));
        }
        let taken_coins = self.take_own_coins(amount)?;
        let mut receiver = self.accounts.entry(to)?.or_insert_default()?;
        receiver.give(taken_coins)?;

        Ok(())
    }

    #[call]
    pub fn take_as_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self.take_own_coins(amount)?;

        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?;

        paid.give::<S, _>(taken_coins.amount)
    }

    fn take_own_coins(&mut self, amount: Amount) -> Result<Coin<S>> {
        let signer = self.signer()?;

        let taken_coins = self
            .accounts
            .get_mut(signer)?
            .ok_or_else(|| Error::Coins("Insufficient funds".into()))?
            .take(amount)?;

        Ok(taken_coins)
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Signer("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Unauthorized account action".into()))
    }

    #[call]
    pub fn give_from_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?
            .take(amount)?;

        self.give_own_coins(taken_coins)
    }

    #[call]
    pub fn give_from_funding_all(&mut self) -> Result<()> {
        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?;
        let balance = paid.balance::<S>()?;
        let taken_coins = paid.take(balance)?;

        self.give_own_coins(taken_coins)
    }

    fn give_own_coins(&mut self, coins: Coin<S>) -> Result<()> {
        let signer = self.signer()?;

        self.accounts
            .entry(signer)?
            .or_insert_default()?
            .give(coins)?;

        Ok(())
    }

    #[query]
    pub fn balance(&self, address: Address) -> Result<Amount> {
        match self.accounts.get(address)? {
            Some(coin) => Ok(coin.amount),
            None => Ok(0.into()),
        }
    }

    #[query]
    pub fn exists(&self, address: Address) -> Result<bool> {
        Ok(self.accounts.get(address)?.is_some())
    }

    pub fn allow_transfers(&mut self, enabled: bool) {
        self.transfers_allowed = enabled;
    }

    pub fn add_transfer_exception(&mut self, address: Address) -> Result<()> {
        self.transfer_exceptions.insert(address, ())
    }

    pub fn deposit(&mut self, address: Address, coins: Coin<S>) -> Result<()> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.give(coins)?;

        Ok(())
    }

    pub fn withdraw(&mut self, address: Address, amount: Amount) -> Result<Coin<S>> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.take(amount)
    }

    pub fn accounts(&self) -> &Map<Address, Coin<S>> {
        &self.accounts
    }
}

#[cfg(feature = "abci")]
impl<S: Symbol, T: v1::coins::Symbol> Migrate<v1::coins::Accounts<T>> for Accounts<S> {
    fn migrate(&mut self, legacy: v1::coins::Accounts<T>) -> Result<()> {
        let accounts = legacy.accounts();
        for entry in accounts.iter().unwrap() {
            let (addr, coins) = entry.unwrap();
            let amt: u64 = coins.amount.into();
            if amt > 0 {
                self.deposit(addr.bytes().into(), amt.into())?;
            }
        }

        Ok(())
    }
}
