//! State migration for versioned data.

pub use crate::macros::Migrate;
use crate::{
    encoding::{Decode, Terminated},
    state::State,
    store::Store,
    Error, Result,
};
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

/// Load state data for this type, migrating from a previous version
/// if necessary.
pub trait Migrate: State {
    /// Migrate state data to the current version.
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        let mut value = Self::load(src, bytes)?;
        value.attach(dest)?;
        Ok(value)
    }
}

/// Create a migrated instance of this type from a loaded value of a previous
/// version.
pub trait MigrateFrom<T>: State {
    /// Migrate from the previous version instance.
    fn migrate_from(value: T) -> Result<Self>;
}

/// Migrate a previous version of this type into the current version..
///
/// One should avoid implementing [MigrateInto] and implement [MigrateFrom]
/// instead. Implementing [MigrateFrom] automatically provides one with an
/// implementation of [MigrateInto] thanks to a blanket implementation in orga.
pub trait MigrateInto<T> {
    /// Migrate into the current version.
    fn migrate_into(self) -> Result<T>;
}

impl<T: MigrateFrom<U>, U> MigrateInto<T> for U {
    fn migrate_into(self) -> Result<T> {
        T::migrate_from(self)
    }
}

impl<T: From<U> + State, U> MigrateFrom<U> for T {
    fn migrate_from(value: U) -> Result<Self> {
        Ok(value.into())
    }
}

macro_rules! migrate_impl {
    ($type:ty) => {
        impl Migrate for $type {}
    };
}

migrate_impl!(u8);
migrate_impl!(u16);
migrate_impl!(u32);
migrate_impl!(u64);
migrate_impl!(u128);
migrate_impl!(i8);
migrate_impl!(i16);
migrate_impl!(i32);
migrate_impl!(i64);
migrate_impl!(i128);
migrate_impl!(bool);
migrate_impl!(());

impl<T: Migrate> Migrate for Option<T> {
    #[inline]
    fn migrate(src: Store, dest: Store, mut bytes: &mut &[u8]) -> Result<Self> {
        let variant_byte = u8::decode(&mut bytes)?;
        if variant_byte == 0 {
            Ok(None)
        } else {
            Ok(Some(T::migrate(src, dest, bytes)?))
        }
    }
}

impl<T: Migrate + Terminated, const N: usize> Migrate for [T; N] {
    #[inline]
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        let items: Vec<T> = (0..N)
            .map(|i| {
                let prefix = crate::state::varint(i, N);
                let sub_src = src.sub(prefix.as_slice());
                let sub_dest = dest.sub(prefix.as_slice());
                let value = T::migrate(sub_src, sub_dest, bytes)?;
                Ok(value)
            })
            .collect::<Result<_>>()?;

        items
            .try_into()
            .map_err(|_| Error::State(format!("Cannot convert Vec to array of length {}", N)))
    }
}

impl<T: Migrate + Terminated> Migrate for Vec<T> {
    #[inline]
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        let mut value = vec![];
        while !bytes.is_empty() {
            let prefix = (value.len() as u64).to_be_bytes();
            let sub_src = src.sub(prefix.as_slice());
            let sub_dest = dest.sub(prefix.as_slice());
            let item = T::migrate(sub_src, sub_dest, bytes)?;
            value.push(item);
        }

        Ok(value)
    }
}

impl<T: Migrate> Migrate for RefCell<T> {
    #[inline]
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        Ok(RefCell::new(T::migrate(src, dest, bytes)?))
    }
}

impl<T: 'static> Migrate for PhantomData<T> {}

macro_rules! migrate_tuple_impl {
    ($($type:ident),*; $last_type: ident; $($indices:tt),*) => {
        impl<$($type,)* $last_type> Migrate for ($($type,)* $last_type,)
        where
            $($type: Migrate,)*
            $last_type: Migrate,
            // last type doesn't need to be terminated
            $($type: ed::Terminated,)*
        {
            #[inline]
            fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
                Ok((
                    $(Migrate::migrate(
                        src.sub(&[$indices as u8]),
                        dest.sub(&[$indices as u8]),
                        bytes)?,
                    )*
                ))
            }
        }
    }
}

migrate_tuple_impl!(; A; 0);
migrate_tuple_impl!(A; B; 0, 1);
migrate_tuple_impl!(A, B; C; 0, 1, 2);
migrate_tuple_impl!(A, B, C; D; 0, 1, 2, 3);
migrate_tuple_impl!(A, B, C, D; E; 0, 1, 2, 3, 4);
migrate_tuple_impl!(A, B, C, D, E; F; 0, 1, 2, 3, 4, 5);
migrate_tuple_impl!(A, B, C, D, E, F; G; 0, 1, 2, 3, 4, 5, 6);
migrate_tuple_impl!(A, B, C, D, E, F, G; H; 0, 1, 2, 3, 4, 5, 6, 7);
migrate_tuple_impl!(A, B, C, D, E, F, G, H; I; 0, 1, 2, 3, 4, 5, 6, 7, 8);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I; J; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I, J; K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K; L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);

impl<T: Migrate> Migrate for Rc<T> {
    #[inline]
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        let value = T::migrate(src, dest, bytes)?;
        Ok(Rc::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collections::{Deque, Entry, EntryMap, Map},
        encoding::{Decode, Encode},
        orga,
        state::State,
        store::{BackingStore, MapStore, Read, Shared, Store, Write},
        Result,
    };

    #[orga(version = 2, skip(Migrate))]
    #[derive(Clone, PartialEq, Eq)]
    struct Number {
        #[orga(version(V0))]
        value: u16,
        #[orga(version(V1, V2))]
        value: u32,
    }

    impl Migrate for NumberV1 {
        fn migrate(src: Store, _dest: Store, bytes: &mut &[u8]) -> Result<Self> {
            if bytes[0] == 1 {
                return Self::load(src, bytes);
            }

            let other = NumberV0::load(src, bytes)?;
            let value: u32 = other.value.into();
            Ok(Self { value: value * 2 })
        }
    }

    impl Migrate for NumberV2 {
        fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
            if bytes[0] == 2 {
                return Self::load(src, bytes);
            }

            let other = NumberV1::migrate(src, dest, bytes)?;
            Ok(Self {
                value: other.value * 2,
            })
        }
    }

    #[orga(version = 1, skip(Migrate))]
    #[derive(Entry, Eq, PartialEq)]
    struct NumberEntry {
        #[key]
        index: u8,
        #[orga(version(V0))]
        inner: NumberV0,
        #[orga(version(V1))]
        inner: NumberV1,
    }

    impl Migrate for NumberEntryV1 {
        fn migrate(src: Store, dest: Store, mut bytes: &mut &[u8]) -> Result<Self> {
            if bytes[0] == 1 {
                return Self::load(src, bytes);
            } else {
                *bytes = &bytes[1..];
            }

            Ok(Self {
                index: u8::decode(&mut bytes)?,
                inner: NumberV1::migrate(src.sub(&[1]), dest.sub(&[1]), bytes)?,
            })
        }
    }

    #[orga(version = 1, skip(Migrate))]
    struct Foo {
        #[orga(version(V0))]
        bar: u32,
        #[orga(version(V1))]
        bar: u64,

        #[orga(version(V1))]
        boop: u32,

        #[orga(version(V0))]
        baz: Map<u32, u32>,
        #[orga(version(V1))]
        baz: Map<u32, u32>,

        #[orga(version(V0))]
        beep: Map<NumberV0, Deque<EntryMap<NumberEntryV0>>>,
        #[orga(version(V1))]
        beep: Map<NumberV1, Deque<EntryMap<NumberEntryV1>>>,
    }

    impl Migrate for FooV1 {
        fn migrate(src: Store, dest: Store, mut bytes: &mut &[u8]) -> Result<Self> {
            if bytes[0] == 1 {
                return Self::load(src, bytes);
            } else {
                *bytes = &bytes[1..];
            }

            Ok(Self {
                bar: u32::decode(&mut bytes)?.into(),
                boop: 43,
                baz: Map::migrate(src.sub(&[1]), dest.sub(&[1]), bytes)?,
                beep: Map::migrate(src.sub(&[2]), dest.sub(&[3]), bytes)?,
            })
        }
    }

    #[orga(version = 1)]
    struct WithGeneric<T> {
        a: u32,
        b: T,
    }

    impl<T: State> MigrateFrom<WithGenericV0<T>> for WithGenericV1<T> {
        fn migrate_from(value: WithGenericV0<T>) -> Result<Self> {
            Ok(Self {
                a: value.a,
                b: value.b,
            })
        }
    }

    fn create_foo_v0_store() -> Result<Store> {
        let mut store = Store::new(BackingStore::MapStore(Shared::new(MapStore::new())));

        let mut foo = FooV0 {
            bar: 42,
            ..Default::default()
        };

        foo.baz.insert(12, 34)?;
        foo.baz.insert(789, 1)?;
        foo.baz.insert(1000, 2)?;
        foo.baz.insert(1001, 3)?;
        let key = NumberV0 { value: 10 };
        let mut em = EntryMap::new();
        let entry = NumberEntryV0 {
            index: 11,
            inner: NumberV0 { value: 12 },
        };
        em.insert(entry)?;
        foo.beep.entry(key)?.or_insert_default()?.push_back(em)?;

        let mut bytes = vec![];
        foo.attach(store.clone())?;
        foo.flush(&mut bytes)?;
        store.put(vec![], bytes)?;

        Ok(store)
    }

    #[test]
    fn basic_migration() -> Result<()> {
        let mut store = create_foo_v0_store()?;
        let bytes = store.get(&[])?.unwrap();
        assert_eq!(bytes, vec![0, 0, 0, 0, 42]);
        let foo = FooV0::load(store.clone(), &mut bytes.as_slice())?;

        assert_eq!(foo.bar, 42);
        assert_eq!(foo.baz.get(12)?.unwrap().clone(), 34);
        assert_eq!(store.get(&[1, 0, 0, 0, 12])?.unwrap(), vec![0, 0, 0, 34]);
        // Entry
        assert_eq!(
            store
                .get(&[2, 0, 0, 10, 127, 255, 255, 255, 255, 255, 255, 255, 11])?
                .unwrap(),
            vec![0, 0, 12]
        );
        // Deque meta
        assert_eq!(
            store.get(&[2, 0, 0, 10])?.unwrap(),
            vec![0, 127, 255, 255, 255, 255, 255, 255, 255, 128, 0, 0, 0, 0, 0, 0, 0]
        );
        let key = NumberV0 { value: 10 };
        assert!(foo
            .beep
            .get(key.clone())?
            .unwrap()
            .back()?
            .unwrap()
            .contains_entry_key(NumberEntryV0 {
                index: 11,
                ..Default::default()
            })?);

        let bytes = store.get(&[])?.unwrap();
        let mut foo = FooV1::migrate(store.clone(), store.clone(), &mut bytes.as_slice())?;
        assert_eq!(foo.bar, 42);
        assert_eq!(foo.boop, 43);
        assert_eq!(foo.baz.get(12)?.unwrap().clone(), 34);
        assert!(store.get(&[1, 0, 0, 0, 12])?.is_none());
        assert!(store
            .get(&[2, 0, 0, 10, 127, 255, 255, 255, 255, 255, 255, 255, 11])?
            .is_none());
        assert!(store.get(&[2, 0, 0, 10])?.is_none());

        let key_bytes = key.encode()?;
        let key: NumberV1 = NumberV1::migrate(
            Store::default(),
            Store::default(),
            &mut key_bytes.as_slice(),
        )?;
        assert_eq!(key.encode()?, vec![1, 0, 0, 0, 20]);
        assert_eq!(key.value, 20);
        let entry = foo
            .beep
            .get(key)?
            .unwrap()
            .back()?
            .unwrap()
            .iter()?
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(entry.index, 11);
        assert_eq!(entry.inner.value, 24);

        let mut bytes = vec![];
        foo.attach(store.clone())?;
        foo.flush(&mut bytes)?;
        assert_eq!(bytes, vec![1, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 43]);
        store.put(vec![], bytes)?;
        assert!(store.get(&[1, 0, 0, 0, 12])?.is_none());
        assert_eq!(store.get(&[2, 0, 0, 0, 12])?.unwrap(), vec![0, 0, 0, 34]);
        assert!(store
            .get(&[2, 0, 0, 10, 127, 255, 255, 255, 255, 255, 255, 255, 11])?
            .is_none());
        assert!(store.get(&[2, 0, 0, 10])?.is_none());
        assert_eq!(
            store.get(&[3, 1, 0, 0, 0, 20])?.unwrap(),
            vec![0, 127, 255, 255, 255, 255, 255, 255, 255, 128, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            store
                .get(&[3, 1, 0, 0, 0, 20, 127, 255, 255, 255, 255, 255, 255, 255, 11])?
                .unwrap(),
            vec![1, 0, 0, 0, 24]
        );

        Ok(())
    }
}
