use super::{Amount, Balance, Symbol, Take};
use crate::Result;

pub trait Transfer<S: Symbol, A = Amount>: Take<S, A> {
    fn take_all(&mut self) -> Result<Self::Value>;
}

impl<T, S, A> Transfer<S, A> for T
where
    S: Symbol,
    T: Balance<A> + Take<S, A>,
{
    fn take_all(&mut self) -> Result<T::Value> {
        let balance = self.balance();
        let taken = self.take(balance)?;

        Ok(taken)
    }
}
