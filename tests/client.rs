#![feature(specialization)]

use failure::bail;

use orga::client::{CreateClient, Mock};
use orga::state::State;
use orga::Result;

#[derive(State, Debug)]
pub struct Counter {
    count: u32,
    pub count2: u32,
}

impl Counter {
    pub fn increment(&mut self, n: u32) -> Result<()> {
        if n != self.count {
            bail!("Incorrect count");
        }

        self.count += 1;

        Ok(())
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}

// first expansion

pub struct Client<T> {
    client: T,
    // pub count2: <u32 as ::orga::client::CreateClient<T>>::Client,
}

impl<T: ::orga::client::Client<Counter>> CreateClient<T> for Counter {
    type Client = Client<T>;

    fn create_client(client: T) -> Self::Client {
        Client {
            client,
            // count2: CreateClient::create_client(client.clone()),
        }
    }
}

#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum FieldCall {
    Count2(<u32 as ::orga::call::Call>::Call),
}

impl ::orga::call::Call<::orga::call::Field> for Counter {
    type Call = FieldCall;

    fn call(&mut self, call: FieldCall) -> ::orga::Result<()> {
        match call {
            FieldCall::Count2(subcall) => {
                ::orga::call::Call::<::orga::call::Kind>::call(&mut self.count2, subcall)?
            }
        };

        Ok(())
    }
}

#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum MethodCall {
    Increment(u32),
}

impl ::orga::call::Call<::orga::call::Method> for Counter {
    type Call = MethodCall;

    fn call(&mut self, call: MethodCall) -> ::orga::Result<()> {
        match call {
            MethodCall::Increment(n) => self.increment(n)?,
        };

        Ok(())
    }
}

impl<T: ::orga::client::Client<Counter>> Client<T> {
    fn increment(&mut self, n: u32) -> ::orga::Result<()> {
        self.client
            .call(::orga::call::Item::Method(MethodCall::Increment(n)))
    }

    // fn count(&self) -> Result<u32> {
    //   self.client.query(Counter::Count, |res| match res {
    //     Count(a) => Ok(a),
    //     _ => Err(()),
    //   })
    // }
}

#[test]
fn client() {
    let counter = Counter { count: 0, count2: 0 };
    let (backing_client, counter) = Mock::new(counter);
    let mut client = Counter::create_client(backing_client);

    assert_eq!(
        client.increment(123).unwrap_err().to_string(),
        "Incorrect count",
    );
   
    client.increment(0).unwrap();
    assert_eq!(counter.borrow().count, 1);
    assert_eq!(counter.borrow().count2, 0);

    println!("{:?}", &counter);
}
