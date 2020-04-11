use std::borrow::Borrow;
use std::marker::PhantomData;
use failure::{bail, Fail};
use crate::{Encode, Decode, Store, WrapStore, RefStore};

const EMPTY_KEY: &[u8] = &[];

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Value does not exist")]
    NotFound,

    #[fail(display = "{}", _0)]
    Other(failure::Error)
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Self {
        Error::Other(err)
    }
}

pub struct Value<'a, T: Encode + Decode> {
    store: Box<dyn Store + 'a>,
    value_type: PhantomData<T>
}

impl<'a, T: Encode + Decode> WrapStore<'a> for Value<'a, T> {
    fn wrap_store<S: Store + 'a>(store: S) -> Value<'a, T> {
        Value {
            store: Box::new(store),
            value_type: PhantomData
        }
    }
}

impl<'a, T: Encode + Decode> Value<'a, T> {
    pub fn get(&self) -> Result<T> {
        match self.store.get(EMPTY_KEY)? {
            Some(bytes) => Ok(T::decode(bytes.as_slice())?),
            None => Err(Error::NotFound)
        }
    }

    pub fn set<B: Borrow<T>>(&mut self, value: B) -> Result<()> {
        let bytes = value.borrow().encode()?;
        Ok(self.store.put(EMPTY_KEY.to_vec(), bytes)?)
    }
}

impl<'a, T: Encode + Decode + Default> Value<'a, T> {
    pub fn get_or_default(&self) -> Result<T> {
        match self.get() {
            Ok(value) => Ok(value),
            Err(err) => {
                if let Error::NotFound = err {
                    return Ok(T::default())
                }
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;
    use crate::{MapStore, Read, WrapStore, Store};

    #[test]
    fn u64_wrapper() {
        let mut store = MapStore::new();
        
        {
            let mut n: Value<u64> = Value::wrap_store(store.to_ref());

            assert_eq!(
                n.get().unwrap_err().to_string(),
                "Value does not exist"
            );

            n.set(0x1234567890u64).unwrap();
            assert_eq!(n.get().unwrap(), 0x1234567890);
        }

        assert_eq!(
            store.get(&[]).unwrap(),
            Some(vec![0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90])
        );
    }

    #[test]
    fn default() {
        let mut store = MapStore::new();
        let n: Value<u64> = Value::wrap_store(store.to_ref());
        assert_eq!(n.get_or_default().unwrap(), 0);
    }
}
