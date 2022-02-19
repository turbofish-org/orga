// use crate::call::Call;
// use crate::query::Query;
// use crate::Error;
use crate::Result;

pub use crate::call::Call;
pub use crate::macros::Client;
pub use chain::{CallChain, QueryChain};
pub use mock::Mock;
pub use primitive_client::PrimitiveClient;

mod chain;
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
                PrimitiveClient::new(parent)
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

// TODO: handle arrays of complex types
impl<T, U: Clone, const N: usize> Client<U> for [T; N] {
    type Client = PrimitiveClient<Self, U>;

    fn create_client(parent: U) -> Self::Client {
        PrimitiveClient::new(parent)
    }
}

// TODO: handle vecs of complex types
impl<T, U: Clone> Client<U> for Vec<T> {
    type Client = PrimitiveClient<Self, U>;

    fn create_client(parent: U) -> Self::Client {
        PrimitiveClient::new(parent)
    }
}

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
transparent_impl!(Option<T>);

impl<T: Client<U>, U: Clone, E> Client<U> for std::result::Result<T, E> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

// TODO: move to call module? or will these always be client-specific?
#[async_trait::async_trait(?Send)]
pub trait AsyncCall {
    type Call: Send;

    async fn call(&mut self, call: Self::Call) -> Result<()>;
}

#[async_trait::async_trait(?Send)]
pub trait AsyncQuery {
    type Query;
    type Response;

    async fn query<F, R>(&self, query: Self::Query, check: F) -> Result<R>
    where
        F: FnMut(Self::Response) -> Result<R>;
}

// TODO: support deriving for types inside module in macros, then move this into
// // tests module
// #[derive(Debug, Call, Client, Query)]
// pub struct Foo {
//     pub bar: Bar,
//     pub bar2: Bar,
// }

// impl Foo {
//     #[query]
//     pub fn get_bar(&self, id: u8) -> Result<&Bar> {
//         match id {
//             0 => Ok(&self.bar),
//             1 => Ok(&self.bar2),
//             _ => Err(Error::InvalidID),
//         }
//     }

//     #[call]
//     pub fn get_bar_mut(&mut self, id: u8) -> Result<&mut Bar> {
//         println!("Called get_bar_mut({}) on Foo", id);
//         match id {
//             0 => Ok(&mut self.bar),
//             1 => Ok(&mut self.bar2),
//             _ => Err(Error::InvalidID),
//         }
//     }
// }

// #[derive(Debug, Call, Client, Query)]
// pub struct Bar(u32);

// impl Bar {
//     #[query]
//     pub fn count(&self) -> u32 {
//         self.0
//     }

//     #[call]
//     pub fn increment(&mut self) {
//         println!("called increment() on Bar");
//         self.0 += 1;
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::sync::{Arc, Mutex};

//     #[tokio::test]
//     async fn client() {
//         let state = Arc::new(Mutex::new(Foo {
//             bar: Bar(0),
//             bar2: Bar(0),
//         }));

//         let mut client = Mock::new(state.clone());

//         client.bar.increment().await.unwrap();
//         println!("{:?}\n\n", &state.lock().unwrap());

//         client.get_bar_mut(1).increment().await.unwrap();
//         println!("{:?}\n\n", &state.lock().unwrap());

//         use bar_query::Query as BarQuery;
//         use foo_query::Query as FooQuery;
//         use orga::encoding::Encode;

//         let query = FooQuery::MethodGetBar(1, BarQuery::MethodCount(vec![]).encode().unwrap());
//         let count = client
//             .query(query, |state| Ok(state.get_bar(1)?.count()))
//             .unwrap();
//         println!("{}\n\n", count);
//     }

//     #[ignore]
//     #[tokio::test]
//     #[cfg(feature = "abci")]
//     async fn rpc_client() {
//         use crate::abci::TendermintClient;
//         let mut client = TendermintClient::<Foo>::new("http://localhost:26657").unwrap();

//         client.bar.increment().await.unwrap();
//         client.get_bar_mut(1).increment().await.unwrap();

//         // println!("{:?}\n\n", client.bar.count().await.unwrap());

//         // println!("{:?}\n\n", client.get_bar_count(1).await.unwrap());
//         // println!(
//         //     "{:?}\n\n",
//         //     client.get_bar(1).await.unwrap().count(1).await.unwrap()
//         // );
//         // println!("{:?}\n\n", client.get_bar(1).count().await.unwrap());
//     }
// }
