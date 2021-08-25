use crate::call::{Call};
use crate::Result;

mod mock;

pub use mock::Mock;

pub trait Client<T: Call + ?Sized>: Clone {
  // fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
  //   where F: Fn(T::Res) -> Result<R>;
  
  fn call(&mut self, call: T::Call) -> Result<()>;
}

pub trait CreateClient<T: Client<Self>>: Call {
  type Client;

  fn create_client(backing_client: T) -> Self::Client;
}
