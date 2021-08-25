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

pub struct Client<T: ::orga::client::Client<Counter>> {
    client: T,
    pub count2: <u32 as ::orga::client::CreateClient<Count2Client<T>>>::Client,
}

impl<T: ::orga::client::Client<Counter>> CreateClient<T> for Counter {
    type Client = Client<T>;

    fn create_client(client: T) -> Self::Client {
        Client {
            client: client.clone(),
            count2: CreateClient::create_client(Count2Client {
                client: client.clone(),
            }),
        }
    }
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

#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum FieldQuery {
    Count2(<u32 as ::orga::query::Query>::Query),
}
#[derive(Debug, ::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum FieldRes {
    Count2(<u32 as ::orga::query::Query>::Res),
}
impl ::orga::query::Query<::orga::query::Field> for Counter {
    type Query = FieldQuery;
    type Res = FieldRes;

    fn query(&self, query: FieldQuery) -> ::orga::Result<FieldRes> {
        Ok(match query {
            FieldQuery::Count2(subquery) => FieldRes::Count2(::orga::query::Query::<
                ::orga::query::Kind,
            >::query(
                &self.count2, subquery
            )?),
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
impl ::orga::query::Query<::orga::query::Method> for Counter {
    type Query = MethodQuery;
    type Res = MethodRes;

    fn query(&self, query: MethodQuery) -> ::orga::Result<MethodRes> {
        Ok(match query {
            MethodQuery::Count => MethodRes::Count(self.count()),
        })
    }
}

impl ::orga::query::Query<::orga::query::This> for Counter {
    type Query = ();
    type Res = ();

    fn query(&self, _: ()) -> ::orga::Result<Self::Res> {
        Ok(())
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

    assert_eq!(
        client.increment(123).unwrap_err().to_string(),
        "Incorrect count",
    );

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
