use super::{Adjust, Amount, Balance, Give, Ratio, Symbol, Take};
use crate::call::Call;
use crate::context::GetContext;
use crate::plugins::Paid;
use crate::state::State;
use crate::{Error, Result};
use std::marker::PhantomData;

#[must_use = "If these coins are meant to be discarded, explicitly call the `burn` method"]
#[derive(State, Call, Debug)]
pub struct Coin<S: Symbol> {
    pub amount: Amount,
    symbol: PhantomData<S>,
}

impl<S: Symbol> Default for Coin<S> {
    fn default() -> Self {
        Self {
            amount: Default::default(),
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> Coin<S> {
    pub fn new() -> Self {
        Coin {
            amount: 0.into(),
            symbol: PhantomData,
        }
    }

    pub fn mint<A>(amount: A) -> Self
    where
        A: Into<Amount>,
    {
        Coin {
            amount: amount.into(),
            symbol: PhantomData,
        }
    }

    pub fn transfer<G: Give<S>>(self, dest: &mut G) -> Result<()> {
        dest.add(self.amount)
    }

    pub fn burn(self) {}

    #[call]
    pub fn take_as_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self.take(amount)?;
        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Payable context available".into()))?;

        paid.give::<S, _>(taken_coins.amount)
    }
}

impl<S: Symbol> Balance<S, Amount> for Coin<S> {
    fn balance(&self) -> Amount {
        self.amount
    }
}

impl<S: Symbol> Balance<S, Ratio> for Coin<S> {
    fn balance(&self) -> Ratio {
        self.amount.into()
    }
}

impl<S: Symbol> Adjust for Coin<S> {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        self.amount = (self.amount * multiplier)?.amount();

        Ok(())
    }
}

impl<S: Symbol> Take<S> for Coin<S> {
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount = amount.into();
        self.amount = (self.amount - amount)?;

        Ok(Coin::mint(amount))
    }
}

impl<S: Symbol> Give<S> for Coin<S> {
    fn give(&mut self, value: Coin<S>) -> Result<()> {
        self.amount = (self.amount + value.amount)?;

        Ok(())
    }
}

impl<S: Symbol> From<Amount> for Coin<S> {
    fn from(amount: Amount) -> Self {
        Self::mint(amount)
    }
}

impl<S: Symbol> From<u64> for Coin<S> {
    fn from(amount: u64) -> Self {
        Self::mint(amount)
    }
}
