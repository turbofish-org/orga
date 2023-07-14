use super::{IbcV0, IbcV1};
use crate::{migrate::Migrate, state::State, store::Store};

impl Migrate for IbcV1 {
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        let mut other = IbcV0::migrate(src, dest, bytes)?;

        other
            .root_store
            .remove_range(b"a".to_vec()..b"z".to_vec())?;

        other.local_store.remove_range(..)?;

        let mut out = vec![];
        other.root_store.flush(&mut out)?;
        other.local_store.flush(&mut out)?;

        Ok(Self::default())
    }
}
