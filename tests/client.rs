#![feature(specialization)]
#![feature(trivial_bounds)]

use failure::bail;

use orga::client::{CreateClient, Mock};
use orga::state::State;
use orga::Result;
use orga::collections::Map;

#[derive(State, /*Call, Query, CreateClient*/)]
pub struct Counter {
    count: u32,
    pub count2: u32,
    map: Map<u32, u32>,
}

impl Counter {
    // #[call]
    pub fn increment(&mut self, n: u32) -> Result<()> {
        if n != self.count {
            bail!("Incorrect count");
        }

        self.count += 1;

        Ok(())
    }

    // #[query]
    pub fn count(&self) -> Result<Option<u32>> {
        Ok(Some(self.count))
    }

    // #[query]
    pub fn map(&self) -> Result<&Map<u32, u32>> {
        Ok(&self.map)
    }
}

// first expansion

pub struct Client<C: ::orga::client::ClientFor<Counter>> {
    client: C,
    state: Option<Counter>,
    pub count2: <u32 as ::orga::client::CreateClient<Count2Client<C>>>::Client,
}

impl<C: ::orga::client::ClientFor<Counter>> From<C> for Client<C> {
    fn from(client: C) -> Self {
        Client {
            client: client.clone(),
            state: None,
            count2: Count2Client {
                client: client.clone(),
            }.into(),
        }
    }
}

impl<C: ::orga::client::ClientFor<Counter>> CreateClient<C> for Counter {
    type Client = Client<C>;
}

#[derive(Clone)]
pub struct Count2Client<C: ::orga::client::ClientFor<Counter>> {
    client: C,
}
impl<C: ::orga::client::ClientFor<Counter>> ::orga::client::Client for Count2Client<C> {
    type Query = <u32 as ::orga::query::Query>::Query;
    type QueryRes = u32;

    type Call = <u32 as ::orga::call::Call>::Call;

    fn query<F, R>(&self, query: Self::Query, check: F) -> Result<R>
    where
        F: Fn(&Self::QueryRes) -> Result<R>,
    {
        self.client.query(
            ::orga::query::Item::Field(FieldQuery::Count2(query)),
            |parent| check(&parent.count2),
        )
    }

    fn call(&mut self, call: Self::Call) -> Result<()> {
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
impl ::orga::query::FieldQuery for Counter {
    type Query = FieldQuery;

    fn field_query(&self, query: FieldQuery) -> ::orga::Result<()> {
        use ::orga::query::Query;
        match query {
            FieldQuery::Count2(subquery) => self.count2.query(subquery),
        }
    }
}

#[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum MethodQuery {
    Count,
    Map,
}
#[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
pub enum ChainedQuery {
    Count(<u32 as ::orga::query::Query>::Query),
    Map(<Map<u32, u32> as ::orga::query::Query>::Query),
}
impl ::orga::query::MethodQuery for Counter {
    type Query = MethodQuery;
    type ChainedQuery = ChainedQuery;

    fn method_query(&self, query: MethodQuery) -> ::orga::Result<()> {
        match query {
            MethodQuery::Count => { self.count()?; },
            MethodQuery::Map => { self.map()?; },
        };
        Ok(())
    }

    fn chained_query(&self, query: ChainedQuery) -> ::orga::Result<()> {
        use ::orga::query::Query;
        match query {
            ChainedQuery::Count(query) => self.count().query(query),
            ChainedQuery::Map(query) => self.map().query(query),
        }
    }
}

impl<C: ::orga::client::ClientFor<Counter>> Client<C> {
    fn increment(&mut self, n: u32) -> ::orga::Result<()> {
        self.client
            .call(::orga::call::Item::Method(MethodCall::Increment(n)))
    }

    fn count(&self) -> Result<Option<u32>> {
        self.client.query(
            ::orga::query::Item::Method(MethodQuery::Count),
            |this| this.count(),
        )
    }

    fn map(&self) -> Result<&Map<u32, u32>> {
        todo!();

        // self.client.query(
        //     ::orga::query::Item::Method(MethodQuery::Map),
        //     |this| this.map(),
        // )?;
    }
}

// #[test]
// fn client() {
//     use orga::state::State;

//     let store = orga::store::Shared::new(orga::store::MapStore::new());
//     let store = orga::store::Store::new(store);
//     let counter = Counter::create(store.clone(), ()).unwrap();
    
//     let (backing_client, counter) = Mock::new(counter);
//     let mut client = Counter::create_client(backing_client);
//     assert_eq!(client.count().unwrap(), Some(0));
//     assert_eq!(client.count2.get().unwrap(), 0);
//     // client.count().await?;
//     // client.count2.await?;

//     assert_eq!(
//         client.increment(123).unwrap_err().to_string(),
//         "Incorrect count",
//     );
//     // client.increment(123).await?;

//     client.increment(0).unwrap();
//     assert_eq!(counter.borrow().count, 1);
//     assert_eq!(counter.borrow().count2, 0);
//     assert_eq!(client.count().unwrap(), Some(1));
//     assert_eq!(client.count2.get().unwrap(), 0);

//     counter.borrow_mut().increment(1).unwrap();
//     assert_eq!(counter.borrow().count, 2);
//     assert_eq!(counter.borrow().count2, 0);
//     assert_eq!(client.count().unwrap(), Some(2));
//     assert_eq!(client.count2.get().unwrap(), 0);

//     println!("{:?}", &counter);
// }
