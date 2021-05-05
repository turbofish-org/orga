use super::State;
use crate::error::Result;
use crate::store::{Read, Store};
use std::ops::{Deref, DerefMut};

/// A `State` implementation which exposes the underlying raw store (itself
/// implementing `Store`). If the underlying store is only `Read`, then the
/// `WrapperStore` will only implement `Read`.
///
/// This can be useful when composing `State` types into a hierarchy, when
/// access to the raw `Store` API is still necessary.
pub struct WrapperStore<S: Read>(S);

impl<S: Read> State<S> for WrapperStore<S> {
    fn wrap_store(store: S) -> Result<WrapperStore<S>> {
        Ok(WrapperStore(store))
    }
}

impl<S: Read> Deref for WrapperStore<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &self.0
    }
}

impl<S: Store> DerefMut for WrapperStore<S> {
    fn deref_mut(&mut self) -> &mut S {
        &mut self.0
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::store::*;

//     #[test]
//     fn simple() {
//         let mut store = MapStore::new();
//         let mut wrapper: WrapperStore<_> = (&mut store).wrap().unwrap();

//         assert_eq!(wrapper.get(&[0]).unwrap(), None);
//         wrapper.put(vec![0], vec![1]).unwrap();
//         assert_eq!(wrapper.get(&[0]).unwrap(), Some(vec![1]));
//         assert_eq!(store.get(&[0]).unwrap(), Some(vec![1]));
//     }

//     #[test]
//     fn read_only() {
//         let mut store = MapStore::new();
//         let mut wrapper: WrapperStore<_> = (&mut store).wrap().unwrap();
//         wrapper.put(vec![0], vec![1]).unwrap();

//         let store = store;
//         let wrapper: WrapperStore<_> = store.wrap().unwrap();
//         assert_eq!(wrapper.get(&[0]).unwrap(), Some(vec![1]));
//     }
// }
