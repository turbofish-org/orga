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

// #[cfg(test)]
// mod tests {
//     use serde::{Deserialize, Serialize};

//     use crate::{
//         collections::Map,
//         describe::{Describe, Value},
//         encoding::{Decode, Encode},
//         state::State,
//         store::{DefaultBackingStore, MapStore, Shared, Store, Write},
//         Result,
//     };

//     #[derive(State, Encode, Decode, Describe, Default, Serialize, Deserialize)]
//     struct FooV0 {
//         bar: u32,
//         baz: Map<u32, u32>,
//         beep: Map<u32, Map<bool, u8>>,
//     }

//     fn create_foo_v0_value() -> Result<Value> {
//         let mut store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));

//         let mut foo = FooV0::default();
//         // TODO: remove this prefix after ABCI refactor
//         foo.attach(store.sub(&[0]))?;

//         foo.baz.insert(123, 456)?;
//         foo.baz.insert(789, 1)?;
//         foo.baz.insert(1000, 2)?;
//         foo.baz.insert(1001, 3)?;

//         foo.beep.insert(10, Map::new())?;
//         foo.beep.get_mut(10)?.unwrap().insert(true, 1)?;
//         foo.beep.get_mut(10)?.unwrap().insert(false, 0)?;
//         foo.beep.insert(20, Map::new()).unwrap();
//         foo.beep.get_mut(20)?.unwrap().insert(true, 0)?;
//         foo.beep.get_mut(20)?.unwrap().insert(false, 1)?;

//         foo.flush()?;
//         store.put(vec![], foo.encode()?)?;

//         let mut value = Value::new(foo);
//         // TODO: remove this prefix after ABCI refactor
//         value.attach(store.sub(&[0]))?;

//         Ok(value)
//     }

//     #[test]
//     pub fn migrate_identity() -> Result<()> {
//         let foo = create_foo_v0_value()?;
//         let json = foo.to_json()?.to_string();

//         let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));
//         super::migrate::<_, FooV0>(0, json.as_bytes(), store.clone())?;

//         let mut iter = store.range(..);
//         let mut assert_next = |k: &[u8], v: &[u8]| {
//             assert_eq!(
//                 iter.next().transpose().unwrap(),
//                 Some((k.to_vec(), v.to_vec()))
//             )
//         };
//         assert_next(&[], &[0, 0, 0, 0]);
//         assert_next(&[1, 0, 0, 0, 123], &[0, 0, 1, 200]);
//         assert_next(&[1, 0, 0, 3, 21], &[0, 0, 0, 1]);
//         assert_next(&[1, 0, 0, 3, 232], &[0, 0, 0, 2]);
//         assert_next(&[1, 0, 0, 3, 233], &[0, 0, 0, 3]);
//         assert_next(&[2, 0, 0, 0, 10], &[]);
//         assert_next(&[2, 0, 0, 0, 10, 0], &[0]);
//         assert_next(&[2, 0, 0, 0, 10, 1], &[1]);
//         assert_next(&[2, 0, 0, 0, 20], &[]);
//         assert_next(&[2, 0, 0, 0, 20, 0], &[1]);
//         assert_next(&[2, 0, 0, 0, 20, 1], &[0]);
//         assert_eq!(iter.next().transpose()?, None);

//         Ok(())
//     }

//     #[derive(State, Encode, Decode, Describe, Default, Serialize, Deserialize)]
//     #[state(version = 1, transform = "transform_foo")]
//     struct FooV1 {
//         bar: u64,
//         baz: Map<u32, u32>,
//         beep: Map<u32, Map<u8, u8>>,
//     }

//     fn transform_foo(version: u32, value: &mut crate::JsonValue) -> Result<()> {
//         match version {
//             0 => value["beep"]
//                 .as_array_mut()
//                 .unwrap()
//                 .iter_mut()
//                 .for_each(|arr| {
//                     arr.as_array_mut().unwrap()[1]
//                         .as_array_mut()
//                         .unwrap()
//                         .iter_mut()
//                         .for_each(|arr| {
//                             let key = &mut arr.as_array_mut().unwrap()[0];
//                             *key = serde_json::to_value(match key.as_bool().unwrap() {
//                                 false => 0,
//                                 true => 1,
//                             })
//                             .unwrap();
//                         })
//                 }),
//             1 => {}
//             _ => {
//                 return Err(crate::Error::State(format!(
//                     "Cannot upgrade from version {}",
//                     version
//                 )))
//             }
//         };

//         Ok(())
//     }

//     #[test]
//     fn migrate_transform() -> Result<()> {
//         let foo = create_foo_v0_value()?;
//         let json = foo.to_json()?.to_string();

//         let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));
//         super::migrate::<_, FooV1>(0, json.as_bytes(), store.clone())?;

//         let mut iter = store.range(..);
//         let mut assert_next = |k: &[u8], v: &[u8]| {
//             assert_eq!(
//                 iter.next().transpose().unwrap(),
//                 Some((k.to_vec(), v.to_vec()))
//             )
//         };
//         assert_next(&[], &[0, 0, 0, 0, 0, 0, 0, 0]);
//         assert_next(&[1, 0, 0, 0, 123], &[0, 0, 1, 200]);
//         assert_next(&[1, 0, 0, 3, 21], &[0, 0, 0, 1]);
//         assert_next(&[1, 0, 0, 3, 232], &[0, 0, 0, 2]);
//         assert_next(&[1, 0, 0, 3, 233], &[0, 0, 0, 3]);
//         assert_next(&[2, 0, 0, 0, 10], &[]);
//         assert_next(&[2, 0, 0, 0, 10, 0], &[0]);
//         assert_next(&[2, 0, 0, 0, 10, 1], &[1]);
//         assert_next(&[2, 0, 0, 0, 20], &[]);
//         assert_next(&[2, 0, 0, 0, 20, 0], &[1]);
//         assert_next(&[2, 0, 0, 0, 20, 1], &[0]);
//         assert_eq!(iter.next().transpose()?, None);

//         Ok(())
//     }
// }
