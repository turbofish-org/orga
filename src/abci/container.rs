use super::{BeginBlock, EndBlock, InitChain, Transaction};
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::Result;

pub struct Container<S: State> {
    pub inner: S,
}

#[derive(Encode, Decode)]
pub struct ContainerEncoding<S: State> {
    pub inner: <S as State>::Encoding,
}

impl<S> Default for ContainerEncoding<S>
where
    S: State,
    S::Encoding: Default,
{
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<S> From<Container<S>> for ContainerEncoding<S>
where
    S: State,
    S::Encoding: Default,
{
    fn from(container: Container<S>) -> ContainerEncoding<S> {
        ContainerEncoding {
            inner: container.inner.into(),
        }
    }
}

impl<S> State for Container<S>
where
    S: State,
    S::Encoding: Default,
{
    type Encoding = ContainerEncoding<S>;
    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: S::create(store, data.inner)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        let encoding = ContainerEncoding {
            inner: self.inner.flush()?,
        };
        Ok(encoding)
    }
}

impl<S> BeginBlock for Container<S>
where
    S: State,
    S::Encoding: Default,
{
    fn begin_block(&mut self) -> Result<()> {
        self.inner.begin_block()
    }
}

impl<S> EndBlock for Container<S>
where
    S: State,
    S::Encoding: Default,
{
    fn end_block(&mut self) -> Result<()> {
        self.inner.end_block()
    }
}

impl<S> InitChain for Container<S>
where
    S: State,
    S::Encoding: Default,
{
    fn init_chain(&mut self) -> Result<()> {
        self.inner.init_chain()
    }
}

#[derive(Encode, Decode)]
pub enum ContainerCall {
    Transaction(Transaction),
}
