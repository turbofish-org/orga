use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::query::{self, Query};
use crate::Result;


// mod mock;
// pub use mock::Mock;

pub trait CreateClient<T> {
    type Client;
    
    fn create_client(parent: T) -> Self::Client;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    pub struct Foo {
        pub bar: Bar,
        pub bar2: Bar,
    }
    #[derive(Debug, Encode, Decode)]
    pub enum FooCall {
        Bar(BarCall),
        Bar2(BarCall),
    }
    impl Call for Foo {
        type Call = FooCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?} on Foo", &call);
            match call {
                FooCall::Bar(call) => self.bar.call(call),
                FooCall::Bar2(call) => self.bar2.call(call),
            }
        }
    }
    impl<T: Clone> CreateClient<T> for Foo {
        type Client = FooClient<T>;

        fn create_client(parent: T) -> Self::Client {
            let bar_adapter = BarAdapter { parent: parent.clone() };
            let bar2_adapter = Bar2Adapter { parent: parent.clone() };
            FooClient {
                parent,
                bar: Bar::create_client(bar_adapter),
                bar2: Bar::create_client(bar2_adapter),
            }
        }
    }
    pub struct FooClient<T> {
        parent: T,
        // instance: Option<Foo>,
        pub bar: BarClient<BarAdapter<T>>,
        pub bar2: BarClient<Bar2Adapter<T>>,
    }

    pub struct BarAdapter<T> {
        parent: T,
    }
    impl<T: Call<Call = FooCall>> Call for BarAdapter<T> {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?}) on BarAdapter, wrapping with FooCall::Bar(..)", call);
            self.parent.call(FooCall::Bar(call))
        }
    }

    pub struct Bar2Adapter<T> {
        parent: T,
    }
    impl<T: Call<Call = FooCall>> Call for Bar2Adapter<T> {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?}) on Bar2Adapter, wrapping with FooCall::Bar2(..)", call);
            self.parent.call(FooCall::Bar2(call))
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
    impl<T> CreateClient<T> for Bar {
        type Client = BarClient<T>;

        fn create_client(parent: T) -> Self::Client {
            BarClient { parent }
        }
    }

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
        let mut state = Rc::new(RefCell::new(Foo {
            bar: Bar(0),
            bar2: Bar(0),
        }));
        let mut client = Foo::create_client(state.clone());

        client.bar.increment().unwrap();
        println!("{:?}\n", &state.borrow());

        client.bar2.increment().unwrap();
        println!("{:?}", &state.borrow());
    }
}
