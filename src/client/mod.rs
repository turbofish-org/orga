use failure::bail;

use crate::call::Call;
use crate::query::{self, Query};
use crate::Result;

mod mock;

pub use mock::Mock;

pub trait Client<T: Call + Query + ?Sized>: Clone {
  fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
    where F: Fn(T::Res) -> Result<R>;
  
  fn call(&mut self, call: T::Call) -> Result<()>;
}

pub trait CreateClient<C>: Call + Query + Sized {
  type Client: From<C> + Sized = DefaultClient<Self, C>;

  fn create_client(backing_client: C) -> Self::Client {
    Self::Client::from(backing_client)
  }
}

default impl<T: Call + Query, C> CreateClient<C> for T {}

#[derive(Debug)]
pub struct DefaultClient<T, C> {
  marker: std::marker::PhantomData<T>,
  backing_client: C,
}

impl<T, C: Client<T>, U, V, W, X> DefaultClient<T, C>
where
  T: Query<Query = query::Item<U, V, ()>>,
  T: Query<Res = query::Item<W, X, T>>,
{
  pub fn get(&self) -> Result<T> {
    self.backing_client.query(
      query::Item::This(()),
      |res| match res {
        query::Item::This(t) => Ok(t),
        _ => bail!("Received incorrect result type")
      }
    )
  }
}

impl<T, C> From<C> for DefaultClient<T, C> {
  fn from(client: C) -> Self {
    DefaultClient {
      marker: std::marker::PhantomData,
      backing_client: client,
    }
  }
}

impl<C> CreateClient<C> for u32 {}
