// use crate::Result;
// use std::path::Path;
// use v3::encoding::Decode;
// use v3::merk::MerkStore;
// use v3::state::State;
// use v3::store::{DefaultBackingStore, Read, Shared, Store};

// pub trait Migrate<T: State<S>, S = DefaultBackingStore> {
//     fn migrate(&mut self, legacy: T) -> Result<()>;
// }

// pub fn exec_migration<T: Migrate<U>, U: State, P: AsRef<Path>>(
//     state: &mut T,
//     old_store_path: P,
//     prefix: &[u8],
// ) -> Result<()> {
//     let store =
//         Store::new(Shared::new(MerkStore::new(old_store_path.as_ref().to_path_buf())).into());
//     let data_bytes = store.get(&[]).unwrap().unwrap();
//     let data = <U as State>::Encoding::decode(data_bytes.as_slice()).unwrap();
//     let store = store.sub(prefix);
//     let legacy = U::create(store, data).unwrap();

//     state.migrate(legacy)
// }
