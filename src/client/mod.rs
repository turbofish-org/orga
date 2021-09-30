use std::marker::PhantomData;

use crate::call::Call;
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
#[derive(Debug, Call, Client)]
pub struct Foo {
    pub bar: Bar,
    pub bar2: Bar,
}

impl Foo {
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

#[derive(Debug, Call, Client)]
pub struct Bar(u32);

impl Bar {
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

        let mut client = Foo::create_client(Mock(state.clone()));

        // calls increment on bar, 'increment()' returns a CallChain<u64> which
        // calls on parent (MethodIncrementAdapter) once polled
        client.bar.increment().await.unwrap();
        println!("{:?}\n\n", &state.lock().unwrap());

        client.get_bar_mut(1).increment().await.unwrap();
        println!("{:?}\n\n", &state.lock().unwrap());

        // println!("{:?}\n\n", client.bar.await.unwrap()); // queries state.bar
        // println!("{:?}\n\n", client.bar.count().await.unwrap()); // queries 'this' on return value of count method on state.bar
        // println!("{:?}\n\n", client.get_bar(1).count().await.unwrap()); //
        // // XXX: client.get_bar(1).increment().await.unwrap(); - not possible,
        // // increment requires parent with AsyncCall, MethodGetBarAdapter does
        // // not implement AsyncCall
        // // XXX: client.get_bar_mut(1).count().await.unwrap(); - not possible,
        // // count requires parent with AsyncQuery, MethodGetBarMutAdapter does
        // // not implement AsyncQuery
        // let bar = client.get_bar(1).await.unwrap(); // queries 'this' on return value of get_bar method
        // let count = bar.count().await.unwrap(); // queries 'this' on return value of count method on bar instance
        // println!("{:?} {}\n\n", bar, count);
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
