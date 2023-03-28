use serde::Serialize;

use super::{Amount, Balance, Coin, Decimal, Give, Symbol, Take};
use crate::collections::Map;
use crate::state::State;
use crate::{Error, Result};

#[derive(State, Serialize)]
pub struct MultiShare {
    pub shares: Map<u8, Decimal>,
}

impl MultiShare {
    pub fn amounts(&self) -> Result<Vec<(u8, Amount)>> {
        self.shares
            .iter()?
            .map(|entry| {
                let (denom, amount) = entry?;
                Ok((*denom, amount.amount()?))
            })
            .collect()
    }

    pub fn deduct<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        let amount: Amount = amount.into();

        let mut entry = self.shares.entry(denom)?.or_default()?;
        if *entry < amount {
            return Err(Error::Coins("Insufficient balance".into()));
        }
        *entry = (*entry - amount)?;

        Ok(())
    }
}

impl<S: Symbol> Balance<S, Amount> for MultiShare {
    fn balance(&self) -> Result<Amount> {
        self.shares.get_or_default(S::INDEX)?.amount()
    }
}

impl<S: Symbol> Balance<S, Decimal> for MultiShare {
    fn balance(&self) -> Result<Decimal> {
        Ok(*self.shares.get_or_default(S::INDEX)?)
    }
}

impl<S: Symbol> Take<S, Amount> for MultiShare {
    type Value = Coin<S>;
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount = amount.into();
        self.deduct(amount, S::INDEX)?;

        Ok(S::mint(amount))
    }
}

impl<S: Symbol> Give<Coin<S>> for MultiShare {
    fn give(&mut self, coin: Coin<S>) -> Result<()> {
        let mut entry = self.shares.entry(S::INDEX)?.or_default()?;
        *entry = (*entry + coin.amount)?;

        Ok(())
    }
}

impl Give<(u8, Amount)> for MultiShare {
    fn give(&mut self, coin: (u8, Amount)) -> Result<()> {
        let mut entry = self.shares.entry(coin.0)?.or_default()?;
        *entry = (*entry + coin.1)?;

        Ok(())
    }
}
