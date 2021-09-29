use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use crate::call::Call;
use crate::Result;

pub use crate::macros::Client;

pub trait Client<T: Clone> {
    type Client;

    fn create_client(parent: T) -> Self::Client;
}

#[must_use]
pub struct PrimitiveClient<'a, T, U: Clone + AsyncCall<Call = ()>> {
    parent: U,
    fut: Option<Pin<Box<U::Future<'a>>>>,
    marker: PhantomData<T>,
}

impl<'a, T, U: Clone + AsyncCall<Call = ()>> Clone for PrimitiveClient<'a, T, U> {
    fn clone(&self) -> Self {
        PrimitiveClient {
            parent: self.parent.clone(),
            fut: None,
            marker: PhantomData,
        }
    }
}

impl<'a, T, U: Clone + AsyncCall<Call = ()>> Future for PrimitiveClient<'a, T, U> {
    type Output = Result<()>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output>{
        unsafe {
            let this = self.get_unchecked_mut();

            if this.fut.is_none() {
                // make call, populate future to maybe be polled later
                this.fut = Some(Box::pin(this.parent.call(())));
            }

            // TODO: if future is ready, should we clear the future field so
            // consumer can make additional calls?

            let fut = this.fut.as_mut().unwrap().as_mut();
            fut.poll(cx)
        }
    }
}

macro_rules! primitive_impl {
    ( $x:ty ) => {
        impl<T: Clone + AsyncCall<Call = ()>> Client<T> for $x {
            type Client = PrimitiveClient<'static, $x, T>;
        
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
        impl<T, U> Client<U> for $x
        where
            T: Client<U> + Call,
            U: Clone + AsyncCall<Call = T::Call>,
        {
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

// TODO: move to call module? or will this always be client-specific?
pub trait AsyncCall {
    type Call;
    type Future<'a>: Future<Output = Result<()>> + Send;

    fn call(&mut self, call: Self::Call) -> Self::Future<'_>;
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use futures_lite::future;
    use std::cell::RefCell;
    use std::rc::Rc;

    pub struct Mock<T>(pub Rc<RefCell<T>>);

    impl<T> Clone for Mock<T> {
        fn clone(&self) -> Self {
            Mock(self.0.clone())
        }
    }

    impl<T: Call> AsyncCall for Mock<T> {
        type Call = T::Call;
        type Future<'a> = future::Ready<Result<()>>;

        fn call(&mut self, call: Self::Call) -> Self::Future<'_> {
            let res = self.0.borrow_mut().call(call);
            future::ready(res)
        }
    }
}

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
    use std::cell::RefCell;
    use std::rc::Rc;

    #[ignore]
    #[tokio::test]
    async fn client() {
        let state = Rc::new(RefCell::new(Foo {
            bar: Bar(0),
            bar2: Bar(0),
        }));

        use crate::abci::TendermintClient;
        let mut client = TendermintClient::<Foo>::new("http://localhost:26657").unwrap();

        client.bar.increment().await.unwrap();
        println!("{:?}\n\n", &state.borrow());

        client.get_bar_mut(1).increment().await.unwrap();
        println!("{:?}\n\n", &state.borrow());

        // println!("{:?}\n\n", client.bar.count().await.unwrap());

        // println!("{:?}\n\n", client.get_bar_count(1).await.unwrap());
        // println!(
        //     "{:?}\n\n",
        //     client.get_bar(1).await.unwrap().count(1).await.unwrap()
        // );
        // println!("{:?}\n\n", client.get_bar(1).count().await.unwrap());
    }
}
