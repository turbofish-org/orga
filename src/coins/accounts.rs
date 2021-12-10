use crate::call::Call;
use crate::client::{AsyncCall, CallChain, Client};
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

// pub struct AccountsClient<S, U> {
//     parent: U,
//     _symbol: std::marker::PhantomData<S>,
// }

// impl<S: Symbol, U: Clone> Clone for AccountsClient<S, U> {
//     fn clone(&self) -> Self {
//         AccountsClient {
//             parent: self.parent.clone(),
//             _symbol: std::marker::PhantomData,
//         }
//     }
// }

// impl<S: Symbol, U: Clone> Client<U> for Accounts<S> {
//     type Client = AccountsClient<S, U>;

//     fn create_client(parent: U) -> Self::Client {
//         AccountsClient {
//             parent,
//             _symbol: std::marker::PhantomData,
//         }
//     }
// }

// type AccountsCall<S> = <Accounts<S> as Call>::Call;
// impl<S: Symbol, U: Clone + AsyncCall<Call = <Accounts<S> as Call>::Call>> AccountsClient<S, U> {
//     pub fn transfer(&mut self, to: Address, amount: Amount) -> CallChain<> {
//         let call = AccountsCall::<S>::MethodTransfer(to, amount, vec![]);
//         self.parent.call(call)
//     }
// }
