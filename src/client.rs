use std::cell::RefCell;
use std::ops::{Deref, Not};
use std::rc::Rc;

use crate::Result;
use crate::call::Call;
use crate::query::Query;

pub trait Client<T: Clone> {
    type Client: Clone;

    fn create_client(parent: T) -> Self::Client;
}

impl<T: Clone> Client<T> for () {
    type Client = T;

    fn create_client(parent: T) -> Self::Client {
        parent
    }
}

impl<T: Client<U>, U: Clone> Client<U> for &T {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

impl<T: Client<U>, U: Clone> Client<U> for &mut T {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

impl<T: Client<U>, U: Clone> Client<U> for Result<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

impl<T: Client<U>, U: Clone> Client<U> for Option<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(parent)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use super::*;
    use crate::call::Call;
    use crate::query::Query;
    use crate::encoding::{Decode, Encode};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug)]
    pub struct Foo {
        pub bar: Bar,
        pub bar2: Bar,
    }
    impl Foo {
        pub fn get_bar(&self, id: u8) -> Result<&Bar> {
            println!("Called get_bar({}) on Foo", id);
            match id {
                0 => Ok(&self.bar),
                1 => Ok(&self.bar2),
                _ => failure::bail!("Invalid id"),
            }
        }

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
        GetBarMut(u8, BarCall),
    }
    impl Call for Foo {
        type Call = FooCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!("called call({:?}) on Foo", &call);
            match call {
                FooCall::Bar(call) => self.bar.call(call),
                FooCall::Bar2(call) => self.bar2.call(call),
                FooCall::GetBarMut(id, call) => self.get_bar_mut(id).call(call),
            }
        }
    }
    #[derive(Debug, Encode, Decode)]
    pub enum FooQuery {
        Bar(BarQuery),
        Bar2(BarQuery),
        GetBar(u8, BarQuery),
    }
    impl Query for Foo {
        type Query = FooQuery;

        fn query(&self, query: Self::Query) -> Result<()> {
            println!("called query({:?}) on Foo", &query);
            match query {
                FooQuery::Bar(query) => self.bar.query(query),
                FooQuery::Bar2(query) => self.bar2.query(query),
                FooQuery::GetBar(id, query) => self.get_bar(id).query(query),
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
        pub bar: BarClient<BarAdapter<T>>,
        pub bar2: BarClient<Bar2Adapter<T>>,
    }
    impl<T: Call<Call = FooCall> + Clone> FooClient<T> {
        pub fn get_bar_mut(
            &mut self,
            id: u8,
        ) -> <Result<&mut Bar> as Client<GetBarMutAdapter<T>>>::Client {
            println!("called get_bar_mut({}) on FooClient", id);
            let adapter = GetBarMutAdapter {
                args: (id,),
                parent: self.parent.clone(),
            };
            <Result<&mut Bar> as Client<GetBarMutAdapter<T>>>::create_client(adapter)
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
    pub struct GetBarMutAdapter<T> {
        args: (u8,),
        parent: T,
    }
    impl<T: Call<Call = FooCall>> Call for GetBarMutAdapter<T> {
        type Call = BarCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            println!(
                "called call({:?}) on GetBarMutAdapter, wrapping with FooCall::GetBarMut({}, ..)",
                &call, self.args.0
            );
            self.parent.call(FooCall::GetBarMut(self.args.0, call))
        }
    }

    #[derive(Debug)]
    pub struct Bar(u32);
    impl Bar {
        pub fn increment(&mut self) {
            println!("called increment() on Bar");
            self.0 += 1;
        }

        pub fn count(&self) -> u32 {
            println!("called count() on Bar");
            self.0
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
    #[derive(Debug, Encode, Decode)]
    pub enum BarQuery {
        Count,
    }
    impl Query for Bar {
        type Query = BarQuery;

        fn query(&self, query: Self::Query) -> Result<()> {
            println!("called query({:?}) on Bar", &query);
            match query {
                BarQuery::Count => self.count(),
            };
            Ok(())
        }
    }
    impl<T: Clone> Client<T> for Bar {
        type Client = BarClient<T>;

        fn create_client(parent: T) -> Self::Client {
            BarClient { parent }
        }
    }

    #[derive(Clone)]
    pub struct BarClient<T> {
        parent: T,
    }
    impl<T: Call<Call = BarCall>> BarClient<T> {
        pub fn increment(&mut self) -> Result<()> {
            println!("called increment() on BarClient");
            self.parent.call(BarCall::Increment)
        }
    }

    #[test]
    fn client() {
        let state = Rc::new(RefCell::new(Foo {
            bar: Bar(0),
            bar2: Bar(0),
        }));
        let mut client = Foo::create_client(state.clone());

        client.bar.increment().unwrap();
        println!("{:?}\n\n", &state.borrow());

        client.get_bar_mut(1).increment().unwrap();
        println!("{:?}\n\n", &state.borrow());

        // println!("{:?}\n\n", client.bar.count());

        // // println!("{:?}\n\n", client.get_bar_count(1));
        // println!("{:?}\n\n", client.get_bar(1).count());
    }
}
