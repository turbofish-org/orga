//! Basic single-asset account system.
use crate::coins::{Address, Amount, Coin, Give, Symbol, Take};
use crate::collections::map::Iter as MapIter;
use crate::collections::Map;
use crate::context::GetContext;
use crate::orga;
use crate::plugins::Paid;
use crate::plugins::Signer;
use crate::{Error, Result};

/// Manages accounts and their coin balances for a specific symbol.
#[orga]
pub struct Accounts<S: Symbol> {
    /// Whether transfers are allowed.
    transfers_allowed: bool,
    /// Addresses that are exempt from transfer restrictions.
    transfer_exceptions: Map<Address, ()>,
    /// Mapping of addresses to their coin balances.
    accounts: Map<Address, Coin<S>>,
}

#[orga]
impl<S: Symbol> Accounts<S> {
    /// Returns an iterator over the accounts and their balances.
    pub fn iter(&self) -> Result<MapIter<Address, Coin<S>>> {
        self.accounts.iter()
    }

    /// Transfers coins from the signer's account to the specified address.
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

    /// Takes coins from the signer's account and adds them to the [Paid]
    /// context.
    #[call]
    pub fn take_as_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self.take_own_coins(amount)?;

        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?;

        paid.give::<S, _>(taken_coins.amount)
    }

    /// Takes coins from the signer's account.
    fn take_own_coins(&mut self, amount: Amount) -> Result<Coin<S>> {
        let signer = self.signer()?;

        let taken_coins = self
            .accounts
            .get_mut(signer)?
            .ok_or_else(|| Error::Coins("Insufficient funds".into()))?
            .take(amount)?;

        Ok(taken_coins)
    }

    /// Returns the signer's address.
    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Signer("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Unauthorized account action".into()))
    }

    /// Gives coins to the signer's account from the [Paid] context.
    #[call]
    pub fn give_from_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?
            .take(amount)?;

        self.give_own_coins(taken_coins)
    }

    /// Gives all coins from the [Paid] context with symbol `S` to the signer's
    /// account.
    #[call]
    pub fn give_from_funding_all(&mut self) -> Result<()> {
        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?;
        let balance = paid.balance::<S>()?;
        let taken_coins = paid.take(balance)?;

        self.give_own_coins(taken_coins)
    }

    /// Gives coins to the signer's account.
    fn give_own_coins(&mut self, coins: Coin<S>) -> Result<()> {
        let signer = self.signer()?;

        self.accounts
            .entry(signer)?
            .or_insert_default()?
            .give(coins)?;

        Ok(())
    }

    /// Returns the balance of the specified address.
    #[query]
    pub fn balance(&self, address: Address) -> Result<Amount> {
        match self.accounts.get(address)? {
            Some(coin) => Ok(coin.amount),
            None => Ok(0.into()),
        }
    }

    /// Checks if an account exists for the given address.
    #[query]
    pub fn exists(&self, address: Address) -> Result<bool> {
        Ok(self.accounts.get(address)?.is_some())
    }

    /// Allows or disallows transfers for all accounts.
    pub fn allow_transfers(&mut self, enabled: bool) {
        self.transfers_allowed = enabled;
    }

    /// Adds an address to the list of transfer exceptions, allowing it to
    /// transfer funds even when transfers are generally disabled.
    pub fn add_transfer_exception(&mut self, address: Address) -> Result<()> {
        self.transfer_exceptions.insert(address, ())
    }

    /// Deposits coins into the specified address's account.
    pub fn deposit(&mut self, address: Address, coins: Coin<S>) -> Result<()> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.give(coins)?;

        Ok(())
    }

    /// Withdraws coins from the specified address's account.
    pub fn withdraw(&mut self, address: Address, amount: Amount) -> Result<Coin<S>> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.take(amount)
    }
}
