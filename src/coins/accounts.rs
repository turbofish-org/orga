use crate::call::Call;
use crate::client::Client;
use crate::coins::{Address, Amount, Coin, Give, Symbol, Take};
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::Paid;
use crate::plugins::Signer;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};

#[derive(State, Encode, Decode, Call, Query, Client)]
pub struct Accounts<S: Symbol> {
    accounts: Map<Address, Coin<S>>,
}

impl<S: Symbol> Accounts<S> {
    #[call]
    pub fn transfer(&mut self, to: Address, amount: Amount) -> Result<()> {
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

    #[call]
    pub fn fuzz_grant_self_coins(&mut self, _amount: Amount) -> Result<()> {
        let _address = self.signer()?;
        #[cfg(fuzzing)]
        self.deposit(_address, _amount.into())?;

        Ok(())
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

    fn give_own_coins(&mut self, coins: Coin<S>) -> Result<()> {
        let signer = self.signer()?;

        self.accounts
            .get_mut(signer)?
            .ok_or_else(|| Error::Coins("Insufficient funds".into()))?
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

    pub fn deposit(&mut self, address: Address, coins: Coin<S>) -> Result<()> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.give(coins)?;

        Ok(())
    }

    pub fn withdraw(&mut self, address: Address, amount: Amount) -> Result<Coin<S>> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.take(amount)
    }
}
