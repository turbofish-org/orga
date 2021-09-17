use std::cell::RefCell;
use std::rc::Rc;

use crate::Result;
use crate::call::Call;
use crate::query::Query;

use crate::macros::Client;

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

// impl<T: Clone> Client<T> for Foo {
//     type Client = FooClient<T>;

//     fn create_client(parent: T) -> Self::Client {
//         let bar_adapter = BarAdapter {
//             parent: parent.clone(),
//         };
//         let bar2_adapter = Bar2Adapter {
//             parent: parent.clone(),
//         };
//         FooClient {
//             parent,
//             bar: Bar::create_client(bar_adapter),
//             bar2: Bar::create_client(bar2_adapter),
//         }
//     }
// }
// #[derive(Clone)]
// pub struct FooClient<T> {
//     parent: T,
//     pub bar: BarClient<BarAdapter<T>>,
//     pub bar2: BarClient<Bar2Adapter<T>>,
// }
// impl<T: Call<Call = FooCall> + Clone> FooClient<T> {
//     pub fn get_bar_mut(
//         &mut self,
//         id: u8,
//     ) -> <Result<&mut Bar> as Client<GetBarMutAdapter<T>>>::Client {
//         println!("called get_bar_mut({}) on FooClient", id);
//         let adapter = GetBarMutAdapter {
//             args: (id,),
//             parent: self.parent.clone(),
//         };
//         <Result<&mut Bar> as Client<GetBarMutAdapter<T>>>::create_client(adapter)
//     }
// }

// #[derive(Clone)]
// pub struct BarAdapter<T> {
//     parent: T,
// }
// impl<T: Call<Call = FooCall>> Call for BarAdapter<T> {
//     type Call = BarCall;

//     fn call(&mut self, call: Self::Call) -> Result<()> {
//         println!(
//             "called call({:?}) on BarAdapter, wrapping with FooCall::Bar(..)",
//             call
//         );
//         self.parent.call(FooCall::Bar(call))
//     }
// }

// #[derive(Clone)]
// pub struct Bar2Adapter<T> {
//     parent: T,
// }
// impl<T: Call<Call = FooCall>> Call for Bar2Adapter<T> {
//     type Call = BarCall;

//     fn call(&mut self, call: Self::Call) -> Result<()> {
//         println!(
//             "called call({:?}) on Bar2Adapter, wrapping with FooCall::Bar2(..)",
//             call
//         );
//         self.parent.call(FooCall::Bar2(call))
//     }
// }

// #[derive(Clone)]
// pub struct GetBarMutAdapter<T> {
//     args: (u8,),
//     parent: T,
// }
// impl<T: Call<Call = FooCall>> Call for GetBarMutAdapter<T> {
//     type Call = BarCall;

//     fn call(&mut self, call: Self::Call) -> Result<()> {
//         println!(
//             "called call({:?}) on GetBarMutAdapter, wrapping with FooCall::GetBarMut({}, ..)",
//             &call, self.args.0
//         );
//         self.parent.call(FooCall::GetBarMut(self.args.0, call))
//     }
// }

#[derive(Debug, Client, Call)]
pub struct Bar(u32);
impl Bar {
    #[call]
    pub fn increment(&mut self) {
        println!("called increment() on Bar");
        self.0 += 1;
    }

    pub fn count(&self) -> u32 {
        println!("called count() on Bar");
        self.0
    }
}

// #[derive(Clone)]
// pub struct BarClient<T> {
//     parent: T,
// }
// impl<T: Call<Call = BarCall>> BarClient<T> {
//     pub fn increment(&mut self) -> Result<()> {
//         println!("called increment() on BarClient");
//         self.parent.call(BarCall::Increment)
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn client() {
        let state = Rc::new(RefCell::new(Foo {
            bar: Bar(0),
            bar2: Bar(0),
        }));
        let mut client = Foo::create_client(state.clone());

        client.bar;
        // client.bar.increment().unwrap();
        // println!("{:?}\n\n", &state.borrow());

        // client.get_bar_mut(1).increment().unwrap();
        // println!("{:?}\n\n", &state.borrow());

        // println!("{:?}\n\n", client.bar.count());

        // // println!("{:?}\n\n", client.get_bar_count(1));
        // println!("{:?}\n\n", client.get_bar(1).count());
    }
}
