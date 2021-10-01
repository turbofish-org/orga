use std::marker::PhantomData;

use crate::call::Call;
use crate::query::Query;
use crate::Result;

pub use crate::macros::Client;
pub use call_chain::CallChain;
pub use mock::Mock;
pub use primitive_client::PrimitiveClient;

mod call_chain;
mod mock;
mod primitive_client;

pub trait Client<T: Clone> {
    type Client;

    fn create_client(parent: T) -> Self::Client;
}

macro_rules! primitive_impl {
    ( $x:ty ) => {
        impl<T: Clone> Client<T> for $x {
            type Client = PrimitiveClient<$x, T>;

            fn create_client(parent: T) -> Self::Client {
                PrimitiveClient {
                    parent,
                    fut: None,
                    marker: PhantomData,
                }
            }
        }
    };
}

primitive_impl!(());
primitive_impl!(bool);
primitive_impl!(char);
primitive_impl!(u8);
primitive_impl!(u16);
primitive_impl!(u32);
primitive_impl!(u64);
primitive_impl!(u128);
primitive_impl!(i8);
primitive_impl!(i16);
primitive_impl!(i32);
primitive_impl!(i64);
primitive_impl!(i128);

macro_rules! transparent_impl {
    ( $x:ty ) => {
        impl<T: Client<U>, U: Clone> Client<U> for $x {
            type Client = T::Client;

            fn create_client(parent: U) -> Self::Client {
                T::create_client(parent)
            }
        }
    };
}

transparent_impl!(&T);
transparent_impl!(&mut T);
transparent_impl!(Result<T>);
transparent_impl!(Option<T>);

// TODO: move to call module? or will these always be client-specific?
#[async_trait::async_trait]
pub trait AsyncCall {
    type Call;

    async fn call(&mut self, call: Self::Call) -> Result<()>;
}

#[async_trait::async_trait]
pub trait AsyncQuery {
    type Query;
    type Response;

    async fn query<F>(&self, query: Self::Query, check: F) -> Result<&Self::Response>
    where
        F: FnMut(&Self::Response) -> Result<()>;
}

// TODO: support deriving for types inside module in macros, then move this into
// tests module
#[derive(Debug, Call, Client, Query)]
pub struct Foo {
    pub bar: Bar,
    pub bar2: Bar,
}

impl Foo {
    #[query]
    pub fn get_bar(&self, id: u8) -> Result<&Bar> {
        match id {
            0 => Ok(&self.bar),
            1 => Ok(&self.bar2),
            _ => failure::bail!("Invalid id"),
        }
    }

    #[call]
    pub fn get_bar_mut(&mut self, id: u8) -> Result<&mut Bar> {
        println!("Called get_bar_mut({}) on Foo", id);
        match id {
            0 => Ok(&mut self.bar),
            1 => Ok(&mut self.bar2),
            _ => failure::bail!("Invalid id"),
        }
    }
}

#[derive(Debug, Call, Client, Query)]
pub struct Bar(u32);

impl Bar {
    #[query]
    pub fn count(&self) -> u32 {
        self.0
    }

    #[call]
    pub fn increment(&mut self) {
        println!("called increment() on Bar");
        self.0 += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn client() {
        let state = Arc::new(Mutex::new(Foo {
            bar: Bar(0),
            bar2: Bar(0),
        }));

        let mut client = Mock::new(state.clone());

        client.bar.increment().await.unwrap();
        println!("{:?}\n\n", &state.lock().unwrap());

        client.get_bar_mut(1).increment().await.unwrap();
        println!("{:?}\n\n", &state.lock().unwrap());

        use bar_query::Query as BarQuery;
        use foo_query::Query as FooQuery;
        use orga::encoding::Encode;
        
        let query = FooQuery::MethodGetBar(1, BarQuery::MethodCount(vec![]).encode().unwrap());
        let count = client
            .query(query, |state| Ok(state.get_bar(1)?.count()))
            .unwrap();
        println!("{}\n\n", count);
    }

    #[ignore]
    #[tokio::test]
    async fn rpc_client() {
        use crate::abci::TendermintClient;
        let mut client = TendermintClient::<Foo>::new("http://localhost:26657").unwrap();

        client.bar.increment().await.unwrap();
        client.get_bar_mut(1).increment().await.unwrap();

        // println!("{:?}\n\n", client.bar.count().await.unwrap());

        // println!("{:?}\n\n", client.get_bar_count(1).await.unwrap());
        // println!(
        //     "{:?}\n\n",
        //     client.get_bar(1).await.unwrap().count(1).await.unwrap()
        // );
        // println!("{:?}\n\n", client.get_bar(1).count().await.unwrap());
    }
}
