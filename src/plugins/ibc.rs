use crate::call::Call as CallTrait;
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::{Address, Symbol};
use crate::encoding::{Decode, Encode};
use crate::ibc::Ibc;
use crate::query::Query as QueryTrait;
use crate::state::State;
use crate::{Error, Result};
use ibc::core::ics26_routing::msgs::Ics26Envelope;
use ibc_proto::google::protobuf::Any;
use prost::Message;
use std::convert::{TryFrom, TryInto};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub struct IbcPlugin<T> {
    inner: T,
}

impl<T> Deref for IbcPlugin<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for IbcPlugin<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: State> State for IbcPlugin<T> {
    type Encoding = (T::Encoding,);

    fn create(store: crate::store::Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store.sub(&[0]), data.0)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<T: State> From<IbcPlugin<T>> for (T::Encoding,) {
    fn from(plugin: IbcPlugin<T>) -> Self {
        (plugin.inner.into(),)
    }
}

pub enum Call<T> {
    Inner(T),
    Ics26(Any),
}

unsafe impl<T> Send for Call<T> {}

impl<T: Encode> Encode for Call<T> {
    fn encoding_length(&self) -> ed::Result<usize> {
        match self {
            Call::Inner(inner) => inner.encoding_length(),
            Call::Ics26(envelope) => Ok(envelope.encoded_len()),
        }
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match self {
            Call::Inner(inner) => inner.encode_into(dest),
            Call::Ics26(envelope) => {
                let bytes = envelope.encode_to_vec();
                dest.write_all(bytes.as_slice())?;

                Ok(())
            }
        }
    }
}

impl<T: Decode> Decode for Call<T> {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        let maybe_any = Any::decode(bytes.clone().as_slice());
        if let Ok(any) = maybe_any {
            if Ics26Envelope::try_from(any).is_ok() {
                Ok(Call::Ics26(any))
            } else {
                let native = T::decode(bytes.as_slice())?;
                Ok(Call::Inner(native))
            }
        } else {
            let native = T::decode(bytes.as_slice())?;
            Ok(Call::Inner(native))
        }
    }
}

impl<T> CallTrait for IbcPlugin<T>
where
    T: CallTrait + State,
    T::Call: Encode + 'static,
{
    type Call = Call<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match call {
            Call::Inner(native) => self.inner.call(native),
            Call::Ics26(envelope) => {
                todo!()
            }
        }
    }
}

#[derive(Encode, Decode)]
pub enum Query<T: QueryTrait> {
    Inner(T::Query),
    Ibc(<Ibc as QueryTrait>::Query),
}

impl<T: QueryTrait> QueryTrait for IbcPlugin<T> {
    type Query = Query<T>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Query::Inner(inner_query) => self.inner.query(inner_query),
            Query::Ibc(ibc_query) => todo!(),
        }
    }
}
