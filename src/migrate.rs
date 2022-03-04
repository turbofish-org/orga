use crate::Result;
use std::path::Path;
use v1::encoding::Decode;
use v1::merk::MerkStore;
use v1::state::State;
use v1::store::{Read, Shared, Store};

pub trait Migrate {
    type Legacy: State;

    fn migrate(&mut self, legacy: Self::Legacy) -> Result<()>;
}

pub fn exec_migration<T: Migrate, P: AsRef<Path>>(
    state: &mut T,
    old_store_path: P,
    prefix: &[u8],
) -> Result<()> {
    let store =
        Store::new(Shared::new(MerkStore::new(old_store_path.as_ref().to_path_buf())).into());
    let data_bytes = store.get(&[]).unwrap().unwrap();
    let data = <T::Legacy as State>::Encoding::decode(data_bytes.as_slice()).unwrap();
    let store = store.sub(prefix);
    let legacy = T::Legacy::create(store, data).unwrap();

    state.migrate(legacy)
}
