use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::query::{self, Query};
use crate::Result;

pub trait Client<T> {
    type Client;

    fn create_client(parent: T) -> Self::Client;
}

impl<T: Client<U>, U> Client<U> for Result<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

impl<T: Client<U>, U> Client<U> for Option<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

impl<T: Client<U>, U> Client<U> for &T {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

impl<T: Client<U>, U> Client<U> for &mut T {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use super::*;
    use crate::collections::{ChildMut, Map};

    #[derive(Debug)]
    pub struct Signer<T> {
        inner: T,
    }
    impl<T: Call> Call for Signer<T>
    where
        T::Call: Debug,
    {
        type Call = (u32, T::Call);

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("Called call({:?}) on Signer", &call);

            let (signature, subcall) = call;
            if signature != 123 {
                failure::bail!("Invalid signature");
            }

            self.inner.call(subcall)
        }
    }
    impl<T: Client<SignerClient<T, U>>, U: Clone> Client<U> for Signer<T> {
        type Client = T::Client;

        fn create_client(parent: U) -> Self::Client {
            T::create_client(SignerClient {
                parent: parent.clone(),
                marker: std::marker::PhantomData,
            })
        }
    }
    pub struct SignerClient<T, U: Clone> {
        parent: U,
        marker: std::marker::PhantomData<T>,
    }
    impl<T, U: Clone> Clone for SignerClient<T, U> {
        fn clone(&self) -> Self {
            SignerClient {
                parent: self.parent.clone(),
                marker: std::marker::PhantomData,
            }
        }
    }
    impl<T: Call, U: Call<Call = (u32, T::Call)> + Clone> Call for SignerClient<T, U>
    where
        T::Call: Debug,
    {
        type Call = T::Call;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?}) on SignerClient, adding signature", &call);
            self.parent.call((123, call))
        }
    }

    #[derive(Debug)]
    pub struct Foo {
        pub bar: Bar,
        pub bar2: Bar,
    }
    impl Foo {
        pub fn get_bar_mut(&mut self, id: u8) -> Result<&mut Bar> {
            println!("Called get_bar_mut({}) on Foo", id);
            match id {
                0 => Ok(&mut self.bar),
                1 => Ok(&mut self.bar2),
                _ => failure::bail!("Invalid id"),
            }
        }
    }
    #[derive(Debug, Encode, Decode)]
    pub enum FooCall {
        Bar(BarCall),
        Bar2(BarCall),
        GetBar(u8, BarCall),
    }
    impl Call for Foo {
        type Call = FooCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?} on Foo", &call);
            match call {
                FooCall::Bar(call) => self.bar.call(call),
                FooCall::Bar2(call) => self.bar2.call(call),
                FooCall::GetBar(id, call) => self.get_bar_mut(id)?.call(call),
            }
        }
    }
    impl<T: Clone> Client<T> for Foo {
        type Client = FooClient<T>;

        fn create_client(parent: T) -> Self::Client {
            let bar_adapter = BarAdapter {
                parent: parent.clone(),
            };
            let bar2_adapter = Bar2Adapter {
                parent: parent.clone(),
            };
            FooClient {
                parent,
                bar: Bar::create_client(bar_adapter),
                bar2: Bar::create_client(bar2_adapter),
            }
        }
    }
    #[derive(Clone)]
    pub struct FooClient<T> {
        parent: T,
        // instance: Option<Foo>,
        pub bar: BarClient<BarAdapter<T>>,
        pub bar2: BarClient<Bar2Adapter<T>>,
    }
    impl<T: Call<Call = FooCall> + Clone> FooClient<T> {
        pub fn get_bar_mut(
            &mut self,
            id: u8,
        ) -> <Result<&mut Bar> as Client<GetBarAdapter<T>>>::Client {
            println!("called get_bar_mut({}) on FooClient", id);
            let adapter = GetBarAdapter {
                args: (id,),
                parent: self.parent.clone(),
            };
            <Result<&mut Bar> as Client<GetBarAdapter<T>>>::create_client(adapter)
        }
    }

    #[derive(Clone)]
    pub struct BarAdapter<T> {
        parent: T,
    }
    impl<T: Call<Call = FooCall>> Call for BarAdapter<T> {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!(
                "called call({:?}) on BarAdapter, wrapping with FooCall::Bar(..)",
                call
            );
            self.parent.call(FooCall::Bar(call))
        }
    }

    #[derive(Clone)]
    pub struct Bar2Adapter<T> {
        parent: T,
    }
    impl<T: Call<Call = FooCall>> Call for Bar2Adapter<T> {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!(
                "called call({:?}) on Bar2Adapter, wrapping with FooCall::Bar2(..)",
                call
            );
            self.parent.call(FooCall::Bar2(call))
        }
    }

    #[derive(Clone)]
    pub struct GetBarAdapter<T> {
        args: (u8,),
        parent: T,
    }
    impl<T: Call<Call = FooCall>> Call for GetBarAdapter<T> {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!(
                "called call({:?}) on GetBarAdapter, wrapping with FooCall::GetBar({}, ..)",
                &call, self.args.0
            );
            self.parent.call(FooCall::GetBar(self.args.0, call))
        }
    }

    #[derive(Debug)]
    pub struct Bar(u32);
    impl Bar {
        pub fn increment(&mut self) {
            println!("called increment() on Bar");
            self.0 += 1;
        }
    }
    #[derive(Debug, Encode, Decode)]
    pub enum BarCall {
        Increment,
    }
    impl Call for Bar {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?}) on Bar", &call);
            match call {
                BarCall::Increment => self.increment(),
            };
            Ok(())
        }
    }
    impl<T> Client<T> for Bar {
        type Client = BarClient<T>;

        fn create_client(parent: T) -> Self::Client {
            BarClient { parent }
        }
    }

    #[derive(Clone)]
    pub struct BarClient<T> {
        parent: T,
        // instance: Option<Bar>,
    }
    impl<T: Call<Call = BarCall>> BarClient<T> {
        pub fn increment(&mut self) -> Result<()> {
            println!("called increment() on BarClient");
            self.parent.call(BarCall::Increment)
        }
    }

    #[test]
    fn client() {
        let mut state = Rc::new(RefCell::new(Signer {
            inner: Foo {
                bar: Bar(0),
                bar2: Bar(0),
            },
        }));
        let mut client = Signer::<Foo>::create_client(state.clone());

        client.bar.increment().unwrap();
        println!("{:?}\n", &state.borrow());

        client.get_bar_mut(1).increment().unwrap();
        println!("{:?}", &state.borrow());
    }
}
