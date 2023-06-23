use super::{IbcV0, IbcV1};
use crate::{migrate::MigrateFrom, state::State};

impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(mut other: IbcV0) -> crate::Result<Self> {
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
