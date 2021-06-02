use crate::store::*;
use crate::Result;

pub mod value;
pub mod wrapper;

// pub use value::Value;
pub use wrapper::WrapperStore;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
pub trait State<S>: Sized {
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    fn create(store: Store<S>, decoded: Self::Encoding) -> Result<Self>
    where
        S: Read;

    fn flush(self) -> Result<Self::Encoding>
    where
        S: Write;
}

impl<S, T: ed::Encode + ed::Decode> State<S> for T {
    type Encoding = Self;

    #[inline]
    fn create(_: Store<S>, value: Self) -> Result<Self> {
        Ok(value)
    }

    #[inline]
    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

mod tests2 {
    // #[derive(State, Query)]
    // struct CounterState {
    //     counts: Map<u64, u64>,
    // }

    // impl CounterState {
    //     pub fn get(&self, id: u64) -> Result<u64> {
    //         Ok(*self.counts.get(id)?.or_default()?)
    //     }
    
    //     pub fn compare_and_increment(&mut self, id: u64, n: u64) -> Result<()> {
    //         let mut count = self.counts
    //             .entry(id)?
    //             .or_default()?;
    //         ensure!(count == tx.count, "Wrong count, gtfo");
    //         count += 1;
    //     }
    // }

    // fn my_state_machine(state: &mut CounterState, tx: Tx) -> Result<()> {
    //     state.compare_and_increment(tx.id, tx.count)?;
    // }

    // fn main() -> App {
    //     App::new("counter", my_state_machine)
    // }

    // type CountedMapEncoding<'a, K: State2<S>, V: State2<S>, S: Read2 + Sub> = (
    //     <u64 as State2<S>>::Encoding,
    //     <Map<'a, K, V, S> as State2<S>>::Encoding,
    // );

    // impl<'a, K, V, S> State2<S> for CountedMap<'a, K, V, S>
    // where
    //     K: State2<S>,
    //     V: State2<S>,
    //     S: Read2 + Sub,
    // {
    //     type Encoding = CountedMapEncoding<'a, K, V, S>;

    //     fn create(store: S, decoded: Self::Encoding) -> crate::Result<Self> {
    //         Ok(Self {
    //             count: State2::create(store.sub(vec![0]), decoded.0)?,
    //             map: State2::create(store.sub(vec![1]), decoded.1)?,
    //         })
    //     }

    //     fn flush(&mut self) -> crate::Result<()>
    //     where
    //         S: Write2,
    //     {
    //         todo!()
    //     }
    // }

    // impl<'a, K: State2<S>, V: State2<S>, S: Read2> From<CountedMap<'a, K, V, S>>
    //     for CountedMapEncoding<'a, K, V, S>
    // {
    //     fn from(state: CountedMap<'a, K, V, S>) -> Self {
    //         (state.count.into(), state.map.into())
    //     }
    // }
}

/// A trait for state types that can have their data queried by a client.
///
/// A `Query` implementation will typically just call existing getter methods,
/// with the trait acting as a generic way to call these methods.
pub trait Query {
    /// The type of value sent from the client to the node which is resolving
    /// the query.
    type Request;

    /// The type of value returned to the client when a query is successfully
    /// resolved.
    type Response;

    /// Gets data from the state based on the incoming request, and returns it.
    ///
    /// This will be called client-side in order to reproduce the state access
    /// in order for the client to fully verify the data.
    fn query(&self, req: Self::Request) -> Result<Self::Response>;

    /// Accesses the underlying store to get the data necessary for the incoming
    /// query.
    ///
    /// This is called on the resolving node in order to know which raw store
    /// data to send back to the client to let the client successfully call
    /// `query`, using an instrumented store type which records which keys are
    /// accessed.
    ///
    /// The default implementation for `resolve` is to simply call `query` and
    /// throw away the response for ease of implementation, but this will
    /// typically mean unnecessary decoding the result type. Implementations may
    /// override `resolve` to more efficiently query the state without the extra
    /// decode step.
    fn resolve(&self, req: Self::Request) -> Result<()> {
        self.query(req)?;
        Ok(())
    }
}
