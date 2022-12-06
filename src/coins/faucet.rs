use super::{Amount, Coin, Decimal, Symbol};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::Time;
use crate::state::State;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::time::Duration;

#[derive(State, Encode, Decode, Default, Serialize, Deserialize)]
pub struct Faucet<S: Symbol> {
    _symbol: PhantomData<S>,
    configured: bool,
    amount_minted: Amount,
    start_seconds: i64,
    multiplier_total: Decimal,
    total_to_mint: Amount,
    period_decay: Decimal,
    seconds_per_period: u64,
    num_periods: u32,
}

impl<S: Symbol> Faucet<S> {
    pub fn configure(&mut self, opts: FaucetOptions) -> Result<()> {
        let mut multiplier_total: Decimal = 1.into();
        let mut running_multiplier: Decimal = 1.into();
        let num_periods = opts.num_periods;
        let period_decay = opts.period_decay;
        for _ in 0..num_periods - 1 {
            running_multiplier = (running_multiplier * period_decay)?;
            multiplier_total = (multiplier_total + running_multiplier)?;
        }

        self.total_to_mint = opts.total_coins;
        self.configured = true;
        self.num_periods = num_periods;
        self.period_decay = opts.period_decay;
        self.start_seconds = opts.start_seconds;
        self.multiplier_total = multiplier_total;
        self.seconds_per_period = opts.period_length.as_secs();

        Ok(())
    }

    pub fn mint(&mut self) -> Result<Coin<S>> {
        if !self.configured {
            return Err(Error::Coins(
                "Faucet must be configured before minting".into(),
            ));
        }
        let current_seconds = self.current_seconds()?;
        let seconds_since_start = current_seconds - self.start_seconds;
        if seconds_since_start <= 0 {
            return Ok(0.into());
        }
        let target = self.target_amount_minted(seconds_since_start)?;
        if target > self.amount_minted {
            let delta = (target - self.amount_minted)?;
            self.amount_minted = target;

            Ok(delta.into())
        } else {
            Ok(0.into())
        }
    }

    fn target_amount_minted(&self, seconds_since_start: i64) -> Result<Amount> {
        let mut total: Decimal = 0.into();
        let mut running_multiplier: Decimal = 1.into();
        for i in 0..self.num_periods {
            let total_to_mint_this_period =
                (self.total_to_mint * running_multiplier / self.multiplier_total)?;
            if seconds_since_start > (i as i64 + 1) * self.seconds_per_period as i64 {
                // This period is over
                total = (total + total_to_mint_this_period)?;
                running_multiplier = (running_multiplier * self.period_decay)?;
            } else {
                // This period is in progress
                let seconds_into_period =
                    seconds_since_start - (i as i64) * self.seconds_per_period as i64;
                let period_fraction = (Amount::new(seconds_into_period as u64)
                    / Amount::new(self.seconds_per_period as u64))?;
                total = (total + period_fraction * total_to_mint_this_period)?;
                break;
            }
        }

        total.amount()
    }

    fn current_seconds(&mut self) -> Result<i64> {
        Ok(self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context".into()))?
            .seconds)
    }
}

pub struct FaucetOptions {
    pub num_periods: u32,
    pub period_length: Duration,
    pub total_coins: Amount,
    pub period_decay: Decimal,
    pub start_seconds: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::encoding::{Decode, Encode};
    use crate::store::{MapStore, Shared, Store};
    use serial_test::serial;

    #[derive(Encode, Decode, Debug, Clone, Default, Serialize, Deserialize)]
    struct Simp;
    impl Symbol for Simp {
        const INDEX: u8 = 0;
    }

    impl State for Simp {
        fn attach(&mut self, store: Store) -> Result<()> {
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    #[serial]
    fn halvenings() -> Result<()> {
        let mut faucet: Faucet<Simp> = Faucet::default();

        let _ = faucet
            .mint()
            .expect_err("Should not be able to mint before configuring");

        let total = 210_000_000;
        faucet.configure(FaucetOptions {
            num_periods: 9,
            period_length: Duration::from_secs(10),
            total_coins: total.into(),
            period_decay: (Amount::new(1) / Amount::new(2))?,
            start_seconds: 10,
        })?;

        let mut minted = vec![];
        for i in 0..23 {
            Context::add(Time::from_seconds(i * 5));
            if i == 6 {
                continue;
            }
            minted.push(faucet.mint()?);
            if i == 12 {
                minted.push(faucet.mint()?);
            }
        }
        let minted_amounts: Vec<u64> = minted.iter().map(|coin| coin.amount.into()).collect();
        assert_eq!(
            minted_amounts,
            vec![
                0, 0, 0, 52602740, 52602739, 26301370, 39452055, 13150685, 6575343, 6575342,
                3287671, 3287671, 0, 1643836, 1643836, 821917, 821918, 410959, 410959, 205480,
                205479, 0, 0
            ]
        );
        assert_eq!(minted_amounts.iter().sum::<u64>(), total);

        Ok(())
    }

    #[test]
    #[serial]
    fn thirdenings() -> Result<()> {
        let mut faucet: Faucet<Simp> = Faucet::default();

        let _ = faucet
            .mint()
            .expect_err("Should not be able to mint before configuring");

        let total = 210_000_000;
        faucet.configure(FaucetOptions {
            num_periods: 9,
            period_length: Duration::from_secs(10),
            total_coins: total.into(),
            period_decay: (Amount::new(2) / Amount::new(3))?,
            start_seconds: 10,
        })?;

        let mut minted = vec![];
        for i in 0..23 {
            Context::add(Time::from_seconds(i * 5));
            if i == 6 {
                continue;
            }
            minted.push(faucet.mint()?);
            if i == 12 {
                minted.push(faucet.mint()?);
            }
        }
        let minted_amounts: Vec<u64> = minted.iter().map(|coin| coin.amount.into()).collect();
        assert_eq!(
            minted_amounts,
            vec![
                0, 0, 0, 35934745, 35934745, 23956497, 39927495, 15970998, 10647332, 10647331,
                7098222, 7098221, 0, 4732148, 4732147, 3154765, 3154765, 2103177, 2103176, 1402118,
                1402118, 0, 0
            ]
        );
        assert_eq!(minted_amounts.iter().sum::<u64>(), total);

        Ok(())
    }
}
