use super::{IbcV0, IbcV1, IbcV2};
use crate::migrate::MigrateFrom;
use crate::state::State;

impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(_value: IbcV0) -> crate::Result<Self> {
        unreachable!()
    }
}

impl MigrateFrom<IbcV1> for IbcV2 {
    fn migrate_from(mut value: IbcV1) -> crate::Result<Self> {
        value
            .root_store
            .remove_range(b"a".to_vec()..b"z".to_vec())?;

        value.local_store.remove_range(..)?;

        let mut out = vec![];
        value.root_store.flush(&mut out)?;
        value.local_store.flush(&mut out)?;

        Ok(Self::default())
    }
}
