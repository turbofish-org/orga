use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::query::{self, Query, FieldQuery, MethodQuery};
use crate::Result;

mod mock;

pub use mock::Mock;

pub trait Client: Clone {
    type Query: Encode + Decode;
    type QueryRes;

    type Call: Encode + Decode;

    fn query<F, R>(&self, query: Self::Query, check: F) -> Result<R>
    where
        F: Fn(&Self::QueryRes) -> Result<R>;

    fn call(&mut self, call: Self::Call) -> Result<()>;
}

pub trait ClientFor<T: Query + Call>
where
    Self: Client<Query = T::Query>,
    Self: Client<QueryRes = T>,
    Self: Client<Call = T::Call>,
{
}

impl<T, U: Query + Call> ClientFor<U> for T
where
    T: Client<Query = U::Query>,
    T: Client<QueryRes = U>,
    T: Client<Call = U::Call>,
{
}

pub trait CreateClient<C>: Sized {
    type Client: From<C>;

    fn create_client(client: C) -> Self::Client {
        Self::Client::from(client)
    }
}

impl<T: Encode + Decode, C> CreateClient<C> for T {
    type Client = DefaultClient<T, C>;
}

pub struct DefaultClient<T, C> {
    marker: std::marker::PhantomData<T>,
    backing_client: C,
}

impl<T: Clone, C> DefaultClient<T, C>
where
    C: Client<Query = query::Item<<T as FieldQuery>::Query, <T as MethodQuery>::Query>>,
    C: Client<QueryRes = T>,
{
    pub fn get(&self) -> Result<T> {
        self.backing_client
            .query(query::Item::This, |res| Ok(res.clone()))
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
