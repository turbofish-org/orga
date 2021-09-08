use crate::abci::App;
use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::Result;

pub struct ABCIProvider<T> {
    inner: T,
}

pub struct ABCI {}

#[derive(Encode, Decode)]
pub enum ABCICall<C> {
    InitChain,
    BeginBlock,
    EndBlock,
    DeliverTx(C),
    CheckTx(C),
}

impl<T: State + App> Call for ABCIProvider<T> {
    type Call = ABCICall<T::Call>;
    fn call(&mut self, call: Self::Call) -> Result<()> {
        use ABCICall::*;
        match call {
            InitChain => self.inner.init_chain(),
            BeginBlock => self.inner.begin_block(),
            EndBlock => self.inner.end_block(),
            DeliverTx(inner_call) => self.inner.call(inner_call),
            CheckTx(inner_call) => self.inner.call(inner_call),
        }
    }
}

impl<T> State for ABCIProvider<T>
where
    T: State,
    T::Encoding: From<T>,
{
    type Encoding = (T::Encoding,);
    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store, data.0)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<T> From<ABCIProvider<T>> for (T::Encoding,)
where
    T: State,
    T::Encoding: From<T>,
{
    fn from(provider: ABCIProvider<T>) -> Self {
        (provider.inner.into(),)
    }
}