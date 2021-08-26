#![feature(specialization)]
#![feature(trivial_bounds)]

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

pub struct Client<T: ::orga::client::Client<Counter>> {
    client: T,
    pub count2: <u32 as ::orga::client::CreateClient<Count2Client<T>>>::Client,
}

impl<T: ::orga::client::Client<Counter>> From<T> for Client<T> {
    fn from(client: T) -> Self {
        Client {
            client: client.clone(),
            count2: u32::create_client(Count2Client {
                client: client.clone(),
            }),
        }
    }
}

impl<T: ::orga::client::Client<Counter>> CreateClient<T> for Counter {
    type Client = Client<T>;
}

#[derive(Clone)]
pub struct Count2Client<T> {
    client: T,
}
impl<T: ::orga::client::Client<Counter>> ::orga::client::Client<u32> for Count2Client<T> {
    fn query<F, R>(&self, query: <u32 as ::orga::query::Query>::Query, check: F) -> Result<R>
    where
        F: Fn(<u32 as ::orga::query::Query>::Res) -> Result<R>,
    {
        self.client.query(
            ::orga::query::Item::Field(FieldQuery::Count2(query)),
            |res| match res {
                ::orga::query::Item::Field(FieldRes::Count2(res)) => check(res),
                _ => bail!("Unexpected result"),
            },
        )
    }

    fn call(&mut self, call: <u32 as ::orga::call::Call>::Call) -> Result<()> {
        self.client
            .call(::orga::call::Item::Field(FieldCall::Count2(call)))
    }
}

#[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum FieldCall {
    Count2(<u32 as ::orga::call::Call>::Call),
}
impl ::orga::call::FieldCall for Counter {
    type Call = FieldCall;

    fn field_call(&mut self, call: FieldCall) -> ::orga::Result<()> {
        use ::orga::call::Call;
        match call {
            FieldCall::Count2(subcall) => self.count2.call(subcall)?,
        };

        Ok(())
    }
}

#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum MethodCall {
    Increment(u32),
}
impl ::orga::call::MethodCall for Counter {
    type Call = MethodCall;

    fn method_call(&mut self, call: MethodCall) -> ::orga::Result<()> {
        match call {
            MethodCall::Increment(n) => self.increment(n)?,
        };

        Ok(())
    }
}

#[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum FieldQuery {
    Count2(<u32 as ::orga::query::Query>::Query),
}
#[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum FieldRes {
    Count2(<u32 as ::orga::query::Query>::Res),
}
impl ::orga::query::FieldQuery for Counter {
    type Query = FieldQuery;
    type Res = FieldRes;

    fn field_query(&self, query: FieldQuery) -> ::orga::Result<FieldRes> {
        use ::orga::query::Query;
        Ok(match query {
            FieldQuery::Count2(subquery) => FieldRes::Count2(self.count2.query(subquery)?),
        })
    }
}

#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum MethodQuery {
    Count,
}
#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum MethodRes {
    Count(u32),
}
impl ::orga::query::MethodQuery for Counter {
    type Query = MethodQuery;
    type Res = MethodRes;

    fn method_query(&self, query: MethodQuery) -> ::orga::Result<MethodRes> {
        Ok(match query {
            MethodQuery::Count => MethodRes::Count(self.count()),
        })
    }
}

impl<T: ::orga::client::Client<Counter>> Client<T> {
    fn increment(&mut self, n: u32) -> ::orga::Result<()> {
        self.client
            .call(::orga::call::Item::Method(MethodCall::Increment(n)))
    }

    fn count(&self) -> Result<u32> {
        self.client.query(
            ::orga::query::Item::Method(MethodQuery::Count),
            |res| match res {
                ::orga::query::Item::Method(MethodRes::Count(a)) => Ok(a),
                _ => bail!("Unexpected result"),
            },
        )
    }
}

#[test]
fn client() {
    let counter = Counter {
        count: 0,
        count2: 0,
    };
    let (backing_client, counter) = Mock::new(counter);
    let mut client = Counter::create_client(backing_client);

    assert_eq!(client.count().unwrap(), 0);
    assert_eq!(client.count2.get().unwrap(), 0);
    // client.count().await?;
    // client.count2.await?;

    assert_eq!(
        client.increment(123).unwrap_err().to_string(),
        "Incorrect count",
    );
    // client.increment(123).await?;

    client.increment(0).unwrap();
    assert_eq!(counter.borrow().count, 1);
    assert_eq!(counter.borrow().count2, 0);
    assert_eq!(client.count().unwrap(), 1);
    assert_eq!(client.count2.get().unwrap(), 0);

    counter.borrow_mut().increment(1).unwrap();
    assert_eq!(counter.borrow().count, 2);
    assert_eq!(counter.borrow().count2, 0);
    assert_eq!(client.count().unwrap(), 2);
    assert_eq!(client.count2.get().unwrap(), 0);

    println!("{:?}", &counter);
}
