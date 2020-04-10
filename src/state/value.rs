use std::borrow::Borrow;
use std::marker::PhantomData;
use failure::{bail, Fail};
use crate::{Encode, Decode, Store, WrapStore};

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

pub struct StateValue<'a, T: Encode + Decode> {
    store: &'a mut dyn Store,
    value_type: PhantomData<T>
}

impl<'a, T: Encode + Decode> WrapStore<'a, StateValue<'a, T>> for T {
    fn wrap_store(store: &'a mut dyn Store) -> StateValue<'a, T> {
        StateValue { store, value_type: PhantomData }
    }
}

impl<'a, T: Encode + Decode> StateValue<'a, T> {
    fn get(&self) -> Result<T> {
        match self.store.get(EMPTY_KEY)? {
            Some(bytes) => Ok(T::decode(bytes.as_slice())?),
            None => Err(Error::NotFound)
        }
    }

    fn set<B: Borrow<T>>(&mut self, value: B) -> Result<()> {
        let bytes = value.borrow().encode()?;
        Ok(self.store.put(EMPTY_KEY.to_vec(), bytes)?)
    }
}

impl<'a, T: Encode + Decode + Default> StateValue<'a, T> {
    fn get_or_default(&self) -> Result<T> {
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
    use crate::{MapStore, Read, WrapStore};

    #[test]
    fn u64_wrapper() {
        let mut store = MapStore::new();
        let mut n = u64::wrap_store(&mut store);

        assert_eq!(
            n.get().unwrap_err().to_string(),
            "Value does not exist"
        );

        n.set(0x1234567890u64).unwrap();
        assert_eq!(n.get().unwrap(), 0x1234567890);
        assert_eq!(
            store.get(&[]).unwrap(),
            Some(vec![0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90])
        );
    }

    #[test]
    fn default() {
        let mut store = MapStore::new();
        let mut n = u64::wrap_store(&mut store);
        assert_eq!(n.get_or_default().unwrap(), 0);
    }
}
