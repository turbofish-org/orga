use crate::call::{Call};
use crate::Result;

pub trait Client<T: Call>: Clone {
  type CallRes = ();

  // fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
  //   where F: Fn(T::Res) -> Result<R>;
  
  fn call(&mut self, call: T::Call) -> Result<Self::CallRes>;
}

pub trait CreateClient<T: Client<U>, U: Call> {
  type Client;

  fn create_client(backing_client: T) -> Self::Client;
}
