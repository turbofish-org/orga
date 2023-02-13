pub use crate::macros::MigrateFrom;
use crate::{Error, Result};
use paste::paste;
use std::{cell::RefCell, marker::PhantomData};

pub trait MigrateFrom<T = Self>: Sized {
    fn migrate_from(other: T) -> Result<Self>;
}

pub trait MigrateInto<T>: Sized {
    fn migrate_into(self) -> Result<T>;
}

impl<T, U> MigrateInto<U> for T
where
    U: MigrateFrom<T>,
{
    fn migrate_into(self) -> Result<U> {
        U::migrate_from(self)
    }
}

macro_rules! migrate_from_self_impl {
    ($type:ty) => {
        impl crate::migrate::MigrateFrom<Self> for $type {
            fn migrate_from(other: Self) -> Result<Self> {
                Ok(other)
            }
        }
    };
}
pub(crate) use migrate_from_self_impl;

migrate_from_self_impl!(u8);
migrate_from_self_impl!(u16);
migrate_from_self_impl!(u32);
migrate_from_self_impl!(u64);
migrate_from_self_impl!(u128);
migrate_from_self_impl!(i8);
migrate_from_self_impl!(i16);
migrate_from_self_impl!(i32);
migrate_from_self_impl!(i64);
migrate_from_self_impl!(i128);
migrate_from_self_impl!(bool);
migrate_from_self_impl!(());

impl<T1, T2, const N: usize> MigrateFrom<[T1; N]> for [T2; N]
where
    T2: MigrateFrom<T1>,
{
    fn migrate_from(other: [T1; N]) -> Result<Self> {
        other
            .into_iter()
            .map(MigrateInto::migrate_into)
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| Error::Migrate("Failed to migrate array".into()))
    }
}

impl<T1, T2> MigrateFrom<Vec<T1>> for Vec<T2>
where
    T2: MigrateFrom<T1>,
{
    fn migrate_from(other: Vec<T1>) -> Result<Self> {
        other.into_iter().map(MigrateInto::migrate_into).collect()
    }
}

impl<T1, T2> MigrateFrom<Option<T1>> for Option<T2>
where
    T2: MigrateFrom<T1>,
{
    fn migrate_from(other: Option<T1>) -> Result<Self> {
        match other {
            Some(value) => Ok(Some(value.migrate_into()?)),
            None => Ok(None),
        }
    }
}

impl<T1, T2> MigrateFrom<PhantomData<T1>> for PhantomData<T2> {
    fn migrate_from(_other: PhantomData<T1>) -> Result<Self> {
        Ok(Default::default())
    }
}

impl<T1, T2> MigrateFrom<RefCell<T1>> for RefCell<T2>
where
    T2: MigrateFrom<T1>,
{
    fn migrate_from(other: RefCell<T1>) -> Result<Self> {
        Ok(RefCell::new(other.into_inner().migrate_into()?))
    }
}

macro_rules! migrate_tuple_impl {
        ($($types:ident),* $(,)?; $($indices:tt),*) => {
            paste! {
                impl<$([<$types 1>],)* $([<$types 2>],)*> MigrateFrom<($([<$types 1>],)*)> for ($([<$types 2>],)*)
                where
                    $([<$types 2>]: MigrateFrom<[<$types 1>]>,)*
                {
                    fn migrate_from(other: ($([<$types 1>],)*)) -> Result<($([<$types 2>],)*)> {
                        Ok(($(other.$indices.migrate_into()?,)*))
                    }
                }
            }
        }
}

migrate_tuple_impl!(A; 0);
migrate_tuple_impl!(A, B; 0, 1);
migrate_tuple_impl!(A, B, C; 0, 1, 2);
migrate_tuple_impl!(A, B, C, D; 0, 1, 2, 3);
migrate_tuple_impl!(A, B, C, D, E; 0, 1, 2, 3, 4);
migrate_tuple_impl!(A, B, C, D, E, F; 0, 1, 2, 3, 4, 5);
migrate_tuple_impl!(A, B, C, D, E, F, G; 0, 1, 2, 3, 4, 5, 6);
migrate_tuple_impl!(A, B, C, D, E, F, G, H; 0, 1, 2, 3, 4, 5, 6, 7);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I; 0, 1, 2, 3, 4, 5, 6, 7, 8);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I, J; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
migrate_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K, L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collections::{Deque, Entry, EntryMap, Map},
        encoding::Encode,
        orga,
        state::State,
        store::{DefaultBackingStore, MapStore, Read, Shared, Store, Write},
        Result,
    };

    #[orga(version = 1)]
    #[derive(Clone)]
    struct Number {
        #[orga(version(V0))]
        value: u16,
        #[orga(version(V1))]
        value: u32,
    }
    impl MigrateFrom<NumberV0> for NumberV1 {
        fn migrate_from(other: NumberV0) -> Result<Self> {
            let value: u32 = other.value.into();

            Ok(Self { value: value * 2 })
        }
    }

    #[orga(version = 1)]
    #[derive(Entry)]
    struct NumberEntry {
        #[key]
        index: u8,
        #[orga(version(V0))]
        inner: NumberV0,
        #[orga(version(V1))]
        inner: NumberV1,
    }

    impl MigrateFrom<NumberEntryV0> for NumberEntryV1 {
        fn migrate_from(other: NumberEntryV0) -> Result<Self> {
            Ok(Self {
                index: other.index,
                inner: other.inner.migrate_into()?,
            })
        }
    }

    #[orga(version = 1)]
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

    impl MigrateFrom<FooV0> for FooV1 {
        fn migrate_from(other: FooV0) -> Result<Self> {
            Ok(Self {
                bar: other.bar.try_into().unwrap(),
                boop: 43,
                baz: other.baz.migrate_into()?,
                beep: other.beep.migrate_into()?,
            })
        }
    }

    fn create_foo_v0_store() -> Result<Store> {
        let mut store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));

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

        let mut foo = FooV1::load(store.clone(), &mut bytes.as_slice())?;
        assert_eq!(foo.bar, 42);
        assert_eq!(foo.boop, 43);
        assert_eq!(foo.baz.get(12)?.unwrap().clone(), 34);
        assert!(store.get(&[1, 0, 0, 0, 12])?.is_none());
        assert!(store
            .get(&[2, 0, 0, 10, 127, 255, 255, 255, 255, 255, 255, 255, 11])?
            .is_none());
        assert!(store.get(&[2, 0, 0, 10])?.is_none());
        let key: NumberV1 = key.migrate_into()?;
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
