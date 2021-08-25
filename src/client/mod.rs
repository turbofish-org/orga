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

pub trait CreateClient<T: Client<Self>>: Call + Query {
  type Client;

  fn create_client(backing_client: T) -> Self::Client;
}

pub struct DefaultClient<T, C> {
  marker: std::marker::PhantomData<T>,
  backing_client: C,
}

// impl<T: Call + Query + Clone, C: Client<T>> DefaultClient<T, C> {
//   pub fn get(&self) -> Result<<T as Query<query::This>>::Res> {
//     self.backing_client.query(
//       query::Item::This(()),
//       |res| match res {
//         query::Kind::This(t) => Ok(t),
//         _ => bail!("Received incorrect result type")
//       }
//     )
//   }
// }

impl<C: Client<u32>> DefaultClient<u32, C> {
  pub fn get(&self) -> Result<u32> {
    self.backing_client.query(
      query::Item::This(()),
      |res| match res {
        query::Item::This(t) => Ok(t),
        _ => bail!("Received incorrect result type")
      }
    )
  }
}

impl<C: Client<u32>> CreateClient<C> for u32 {
  type Client = DefaultClient<u32, C>;

  fn create_client(backing_client: C) -> Self::Client {
    DefaultClient {
      marker: std::marker::PhantomData,
      backing_client,
    }
  }
}
